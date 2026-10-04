//! Versioned, portable representation of DASHR's current four-influence mesh input.
use crate::asset::Mesh;
use anyhow::{Context, Result, bail, ensure};
use glam::Mat4;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetDocument {
    pub schema_version: u32,
    pub mesh: Mesh,
}

impl AssetDocument {
    pub const SCHEMA_VERSION: u32 = 1;

    pub fn new(mesh: Mesh) -> Result<Self> {
        let document = Self {
            schema_version: Self::SCHEMA_VERSION,
            mesh,
        };
        document.validate()?;
        Ok(document)
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == Self::SCHEMA_VERSION,
            "unsupported DASHR asset schema version {}; supported version is {}",
            self.schema_version,
            Self::SCHEMA_VERSION
        );
        ensure!(
            self.mesh.vertices.len() >= 3,
            "asset must contain at least three vertices"
        );
        ensure!(
            self.mesh.indices.len() >= 3 && self.mesh.indices.len().is_multiple_of(3),
            "asset indices must contain complete triangles"
        );
        for (vertex_index, vertex) in self.mesh.vertices.iter().enumerate() {
            let mut components = vertex
                .position
                .iter()
                .chain(&vertex.uv)
                .chain(&vertex.weights)
                .chain(&vertex.normal)
                .chain(&vertex.tangent)
                .chain(&vertex.bitangent);
            ensure!(
                components.all(|value| value.is_finite()),
                "vertex {vertex_index} contains a non-finite component"
            );
            let weight_sum = vertex.weights.iter().sum::<f32>();
            ensure!(
                vertex.weights.iter().all(|weight| *weight >= 0.0)
                    && (weight_sum - 1.0).abs() <= 1e-4,
                "vertex {vertex_index} must have nonnegative four-bone weights summing to one"
            );
        }
        for (triangle_index, triangle) in self.mesh.indices.chunks_exact(3).enumerate() {
            for &vertex_index in triangle {
                ensure!(
                    (vertex_index as usize) < self.mesh.vertices.len(),
                    "triangle {triangle_index} references vertex index {vertex_index}, but asset has {} vertices",
                    self.mesh.vertices.len()
                );
            }
            ensure!(
                triangle[0] != triangle[1]
                    && triangle[1] != triangle[2]
                    && triangle[2] != triangle[0],
                "triangle {triangle_index} repeats a vertex index"
            );
        }
        Ok(())
    }

    /// Rejects unsupported intersections and meshes without the renderer's paired seam topology.
    pub fn validate_pose_topology(&self, bones: &[Mat4; 4]) -> Result<()> {
        self.validate()?;
        if let Some(warning) = crate::asset_diagnostics::find_t_junctions(&self.mesh)?.first() {
            bail!(
                "unsupported T-junction: vertex {} lies on edge {:?} of face {} (chart {})",
                warning.vertex_id,
                warning.edge_vertices,
                warning.edge_face_id,
                warning.chart_id
            );
        }
        if let Some(warning) =
            crate::asset_diagnostics::find_self_intersections(&self.mesh, bones)?.first()
        {
            bail!(
                "unsupported posed self-intersection between faces {} (chart {}) and {} (chart {})",
                warning.face_a,
                warning.chart_a,
                warning.face_b,
                warning.chart_b
            );
        }
        crate::topology::validate_indexed_edge_incidence(&self.mesh)?;
        // Match the renderer's bake contract during CPU preflight so open triangle soup fails
        // before a device, material, or GPU resource is created.
        crate::topology::seam_pairs(&self.mesh)?;
        Ok(())
    }

    pub fn from_json(json: &str) -> Result<Self> {
        let document: Self = serde_json::from_str(json).context("decode DASHR asset JSON")?;
        document.validate()?;
        Ok(document)
    }

    pub fn to_json(&self) -> Result<String> {
        self.validate()?;
        serde_json::to_string_pretty(self).context("encode DASHR asset JSON")
    }

    pub fn load(path: &Path) -> Result<Self> {
        let json = std::fs::read_to_string(path)
            .with_context(|| format!("read DASHR asset {}", path.display()))?;
        Self::from_json(&json)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        std::fs::write(path, self.to_json()?)
            .with_context(|| format!("write DASHR asset {}", path.display()))?;
        Ok(())
    }

    pub fn into_mesh(self) -> Mesh {
        self.mesh
    }
}
