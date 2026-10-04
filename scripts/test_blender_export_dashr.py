"""Blender-hosted regression checks for the static DASHR exporter."""

import importlib.util
import json
import math
from collections import defaultdict
from pathlib import Path

import bpy


EXPORTER_PATH = Path(__file__).with_name("blender_export_dashr.py")
SPEC = importlib.util.spec_from_file_location("dashr_blender_export", EXPORTER_PATH)
exporter = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(exporter)


def position_key(position):
    return tuple(round(float(component) * 1e6) for component in position)


def edge_uv_key(start_position, end_position, start_uv, end_uv):
    start = position_key(start_position)
    end = position_key(end_position)
    if start <= end:
        return (start, end), (tuple(start_uv), tuple(end_uv))
    return (end, start), (tuple(end_uv), tuple(start_uv))


def main():
    obj = exporter.create_sphere_fixture(subdivisions=2)
    mesh = obj.data
    assert mesh.polygons and all(len(poly.vertices) == 3 for poly in mesh.polygons)
    assert mesh.uv_layers.active is not None
    assert all(poly.use_smooth for poly in mesh.polygons)

    radii = [vertex.co.length for vertex in mesh.vertices]
    assert max(radii) - min(radii) <= 1e-5
    assert math.isclose(sum(radii) / len(radii), 1.5, rel_tol=1e-5)

    edges = defaultdict(list)
    for polygon in mesh.polygons:
        loop_ids = list(polygon.loop_indices)
        for edge_slot in range(3):
            loop_a = loop_ids[edge_slot]
            loop_b = loop_ids[(edge_slot + 1) % 3]
            vertex_a = mesh.loops[loop_a].vertex_index
            vertex_b = mesh.loops[loop_b].vertex_index
            edge_key, uv_pair = edge_uv_key(
                mesh.vertices[vertex_a].co,
                mesh.vertices[vertex_b].co,
                mesh.uv_layers.active.data[loop_a].uv,
                mesh.uv_layers.active.data[loop_b].uv,
            )
            edges[edge_key].append(uv_pair)
    assert all(len(uses) == 2 for uses in edges.values())
    seam_edges = sum(uses[0] != uses[1] for uses in edges.values())
    assert seam_edges == len(edges)

    encoded, manifest = exporter.export_asset(obj)
    document = json.loads(encoded)
    assert document["schema_version"] == 1
    assert manifest["triangles"] == len(mesh.polygons)
    assert manifest["exported_loop_vertices"] == len(mesh.polygons) * 3
    assert manifest["uv_island_count"] == len(mesh.polygons)
    assert manifest["seam_edge_count"] == seam_edges
    print(
        "B1 sphere export:",
        manifest["triangles"],
        "triangles,",
        manifest["uv_island_count"],
        "UV islands,",
        seam_edges,
        "seam edges",
    )


main()
