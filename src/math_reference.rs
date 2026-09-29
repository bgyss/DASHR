//! Double-precision oracle; deliberately independent of WGSL raster/interpolation.
use anyhow::{Result, ensure};
use glam::{DMat3, DVec2, DVec3};

pub fn inverse_basis(basis: DMat3) -> Result<DMat3> {
    let scale = basis.x_axis.length() * basis.y_axis.length() * basis.z_axis.length();
    ensure!(
        basis.is_finite() && scale.is_finite() && scale > 0.0,
        "non-finite or zero metric basis"
    );
    ensure!(
        basis.determinant().abs() > 1e-10 * scale,
        "singular or ill-conditioned metric basis"
    );
    Ok(basis.inverse())
}

pub fn surface_position(
    inverse: DMat3,
    anchor_or_offset: DVec3,
    point: DVec3,
    uvh: DVec3,
    mode: u32,
) -> DVec3 {
    if mode == 0 {
        inverse * point + anchor_or_offset
    } else {
        inverse * (point - anchor_or_offset) + uvh
    }
}

pub fn gutter_anchor(inverse: DMat3, anchor: DVec3, delta_uv: DVec2) -> Result<DVec3> {
    Ok(anchor + inverse_basis(inverse)? * delta_uv.extend(0.0))
}
