//! Resolution-independent materials and silhouettes, sampled at output resolution.
use crate::{
    game::Kind,
    render::{Rgb, WHITE},
};

fn ellipse(x: f32, y: f32, cx: f32, cy: f32, rx: f32, ry: f32) -> Option<f32> {
    let nx = (x - cx) / rx;
    let ny = (y - cy) / ry;
    let r = nx * nx + ny * ny;
    (r < 1.0).then(|| (1.0 - r).sqrt() * 0.6 + 0.35 - nx * 0.16 - ny * 0.08)
}

pub fn enemy(u: f32, v: f32, body: Rgb, kind: Kind, stride: f32, detail: bool) -> Option<Rgb> {
    let heavy = kind == Kind::Brute;
    let lean = kind == Kind::Runner;
    let chest = if heavy {
        0.31
    } else if lean {
        0.20
    } else {
        0.25
    };
    let steel = Rgb(38, 49, 62);
    let mut out = None;
    // Articulated legs, knee caps and broad boots.
    for sign in [-1.0, 1.0] {
        let lx = 0.5 + sign * (0.13 + stride * (v - 0.65).max(0.0));
        let width = if heavy { 0.12 } else { 0.075 };
        if let Some(light) = ellipse(u, v, lx, 0.79, width, 0.20) {
            out = Some(steel.scale(light));
            if v < 0.73 || (0.79..0.86).contains(&v) {
                out = Some(body.scale(light * 0.85));
            }
            if detail && (v - 0.8).abs() < 0.012 {
                out = Some(WHITE.scale(0.7));
            }
        }
        if let Some(light) = ellipse(u, v, lx + sign * 0.015, 0.96, width * 1.25, 0.045) {
            out = Some(steel.scale(light * 0.85));
        }
        let arm_x = 0.5 + sign * (chest + 0.075 + (v - 0.38).max(0.0) * 0.07);
        if let Some(light) = ellipse(u, v, arm_x, 0.49, if heavy { 0.115 } else { 0.075 }, 0.23) {
            out = Some(body.scale(light * 0.7));
            if (0.43..0.49).contains(&v) || v > 0.63 {
                out = Some(steel.scale(light));
            }
        }
        if let Some(light) = ellipse(
            u,
            v,
            0.5 + sign * chest,
            0.31,
            if heavy { 0.17 } else { 0.11 },
            0.10,
        ) {
            out = Some(body.scale(light));
            if detail && (v - 0.30).abs() < 0.012 {
                out = Some(WHITE.scale(0.6));
            }
        }
    }
    // Ribbed abdomen behind a beveled breastplate.
    if (u - 0.5).abs() < chest * 0.65 && (0.49..0.69).contains(&v) {
        out = Some(steel.scale(if (v * 65.0).fract() < 0.3 { 0.45 } else { 1.0 }));
    }
    if let Some(light) = ellipse(u, v, 0.5, 0.41, chest, 0.19) {
        let seam = (u - 0.5).abs() < 0.008 || (v - 0.47).abs() < 0.009;
        out = Some(if seam { steel } else { body.scale(light) });
        let reactor = ((u - 0.5) / 0.075).powi(2) + ((v - 0.40) / 0.055).powi(2);
        if reactor < 1.0 {
            out = Some(Rgb(255, 215, 118).mix(WHITE, 1.0 - reactor));
        }
        if detail && (0.30..0.34).contains(&v) && (u - 0.5).abs() > 0.09 {
            out = Some(steel.scale(if (u * 60.0).fract() < 0.4 { 0.45 } else { 1.0 }));
        }
    }
    // Helmet dome, recessed visor, respirator, and cheek guards.
    if let Some(light) = ellipse(u, v, 0.5, 0.145, if heavy { 0.18 } else { 0.145 }, 0.13) {
        out = Some(body.scale(light));
        if (0.12..0.17).contains(&v) {
            out = Some(Rgb(255, 225, 148).mix(WHITE, ((0.17 - v) * 12.0).clamp(0.0, 0.6)));
            if detail && (u - 0.5).abs() < 0.01 {
                out = Some(steel);
            }
        } else if v > 0.19 && (u - 0.5).abs() < 0.08 {
            out = Some(steel.scale(if detail && (u * 80.0).fract() < 0.4 {
                0.4
            } else {
                1.0
            }));
        }
    }
    out
}

fn polygon(x: f32, y: f32, points: &[(f32, f32)]) -> bool {
    let mut positive = false;
    let mut negative = false;
    for i in 0..points.len() {
        let (ax, ay) = points[i];
        let (bx, by) = points[(i + 1) % points.len()];
        let cross = (bx - ax) * (y - ay) - (by - ay) * (x - ax);
        positive |= cross > 0.0;
        negative |= cross < 0.0;
    }
    !(positive && negative)
}

// Far muzzle and near receiver share this axis: the player sees the top and
// right side of a forward-pointing shotgun, never looks into its barrel.
pub const MUZZLE: (f32, f32) = (0.30, 0.065);

pub fn weapon(u: f32, v: f32, reload: f32) -> Option<Rgb> {
    let steel = Rgb(100, 124, 140);
    let dark = Rgb(22, 32, 40);
    let glove = Rgb(104, 85, 66);
    let mut out = None;
    // Support arm comes from the lower left, firing arm from the lower right.
    if polygon(
        u,
        v,
        &[(0.04, 1.0), (0.21, 1.0), (0.50, 0.62), (0.40, 0.54)],
    ) {
        out = Some(Rgb(45, 64, 68).scale(0.7 + u));
    }
    if polygon(u, v, &[(0.83, 0.83), (0.97, 0.89), (1.0, 1.0), (0.76, 1.0)]) {
        out = Some(Rgb(42, 59, 63));
    }
    // Narrow distant barrel widens towards the breech, with longitudinal light.
    let axis = MUZZLE.0 + (v - MUZZLE.1) * 0.62;
    let radius = 0.018 + v * 0.055;
    let offset = (u - axis) / radius;
    if (MUZZLE.1..0.57).contains(&v) && offset.abs() < 1.0 {
        out = Some(steel.scale(0.35 + (1.0 - offset * offset).sqrt() * 0.6 - offset * 0.18));
        if offset < -0.65 {
            out = Some(steel.scale(1.2));
        }
    }
    // Sight rib along the top, and a bead at the far end.
    if (0.08..0.53).contains(&v) && (u - axis + radius * 0.18).abs() < 0.008 + v * 0.006 {
        out = Some(dark);
    }
    if (u - MUZZLE.0).abs() < 0.01 && (v - 0.06).abs() < 0.019 {
        out = Some(Rgb(230, 138, 57));
    }
    // Ribbed fore-end sits under the barrel, ahead of the receiver.
    let pv = v - reload * 0.065;
    if polygon(
        u,
        pv,
        &[(0.43, 0.37), (0.49, 0.35), (0.64, 0.59), (0.52, 0.67)],
    ) {
        out = Some(if (pv * 65.0 - u * 15.0).fract().abs() < 0.22 {
            Rgb(36, 32, 29)
        } else {
            Rgb(95, 75, 49).scale(1.15 - u * 0.35)
        });
    }
    // Receiver: top plane, shaded side plane and the ejection port.
    if polygon(
        u,
        v,
        &[(0.52, 0.53), (0.64, 0.48), (0.83, 0.76), (0.67, 0.85)],
    ) {
        out = Some(steel.scale(1.15 - (v - 0.5) * 0.55));
    }
    if polygon(
        u,
        v,
        &[(0.64, 0.48), (0.70, 0.52), (0.88, 0.80), (0.83, 0.76)],
    ) {
        out = Some(steel.scale(0.48));
    }
    if polygon(
        u,
        v,
        &[(0.53, 0.56), (0.55, 0.55), (0.70, 0.80), (0.68, 0.82)],
    ) {
        out = Some(steel.scale(1.4));
    }
    if polygon(
        u,
        v,
        &[(0.64, 0.58), (0.68, 0.56), (0.77, 0.69), (0.72, 0.72)],
    ) {
        out = Some(dark);
    }
    if polygon(
        u,
        v,
        &[(0.65, 0.59), (0.68, 0.58), (0.70, 0.61), (0.67, 0.63)],
    ) {
        out = Some(Rgb(149, 117, 61));
    }
    // Stock continues towards the player's shoulder, beyond the lower edge.
    if polygon(
        u,
        v,
        &[(0.68, 0.83), (0.83, 0.76), (0.99, 1.0), (0.81, 1.0)],
    ) {
        out = Some(dark.scale(1.7 - u * 0.5));
    }
    // Support hand wraps under the fore-end; visible fingers curl along its side.
    let hand_y = 0.63 + reload * 0.065;
    if let Some(light) = ellipse(u, v, 0.43, hand_y, 0.075, 0.08) {
        out = Some(glove.scale(light));
    }
    for i in 0..3 {
        let fy = hand_y - 0.033 + i as f32 * 0.029;
        if let Some(light) = ellipse(u, v, 0.50 + i as f32 * 0.013, fy, 0.044, 0.018) {
            out = Some(glove.scale(light * 0.8));
        }
    }
    // Firing hand wraps the stock wrist behind the action, not the muzzle.
    if let Some(light) = ellipse(u, v, 0.83, 0.89, 0.083, 0.105) {
        out = Some(glove.scale(light));
        if (v * 53.0 - u * 9.0).fract().abs() < 0.13 {
            out = Some(glove.scale(light * 0.65));
        }
    }
    out
}
