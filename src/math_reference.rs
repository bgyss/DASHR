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

/// Specialized cofactor inverse for a 3x3 metric basis; retained as an opt-in math variant.
pub fn inverse_basis_cofactor(basis: DMat3) -> Result<DMat3> {
    let scale = basis.x_axis.length() * basis.y_axis.length() * basis.z_axis.length();
    ensure!(
        basis.is_finite() && scale.is_finite() && scale > 0.0,
        "non-finite or zero metric basis"
    );
    let determinant = basis.determinant();
    ensure!(
        determinant.abs() > 1e-10 * scale,
        "singular or ill-conditioned metric basis"
    );
    let row0 = basis.y_axis.cross(basis.z_axis) / determinant;
    let row1 = basis.z_axis.cross(basis.x_axis) / determinant;
    let row2 = basis.x_axis.cross(basis.y_axis) / determinant;
    Ok(DMat3::from_cols(
        DVec3::new(row0.x, row1.x, row2.x),
        DVec3::new(row0.y, row1.y, row2.y),
        DVec3::new(row0.z, row1.z, row2.z),
    ))
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
