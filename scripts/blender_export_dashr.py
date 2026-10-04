"""Export a static closed Blender mesh as a DASHR AssetDocument v1."""

import argparse
import hashlib
import json
import math
import sys
from collections import Counter
from pathlib import Path

import bpy
from mathutils import Vector


SCHEMA_VERSION = 1


def cli_args():
    argv = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output-dir", required=True, type=Path)
    parser.add_argument("--object", dest="object_name")
    parser.add_argument("--fixture", choices=("torus", "sphere"))
    parser.add_argument("--major-segments", type=int, default=32)
    parser.add_argument("--minor-segments", type=int, default=16)
    parser.add_argument("--subdivisions", type=int, default=2)
    return parser.parse_args(argv)


def create_torus_fixture(major_segments, minor_segments):
    if major_segments < 8 or minor_segments < 8:
        raise ValueError("torus fixture needs at least 8 segments on each ring")
    major_radius = 2.0
    minor_radius = 0.55
    positions = []
    grid = {}
    vertex_parameters = {}
    for major in range(major_segments + 1):
        theta = 2.0 * math.pi * major / major_segments
        for minor in range(minor_segments + 1):
            phi = 2.0 * math.pi * minor / minor_segments
            ring = major_radius + minor_radius * math.cos(phi)
            grid[(major, minor)] = len(positions)
            vertex_parameters[len(positions)] = (major / major_segments, minor / minor_segments)
            positions.append(
                (
                    ring * math.cos(theta),
                    ring * math.sin(theta),
                    minor_radius * math.sin(phi),
                )
            )
    faces = []
    for major in range(major_segments):
        for minor in range(minor_segments):
            faces.append(
                (
                    grid[(major, minor)],
                    grid[(major + 1, minor)],
                    grid[(major + 1, minor + 1)],
                    grid[(major, minor + 1)],
                )
            )

    mesh = bpy.data.meshes.new("DASHR_TorusFixtureMesh")
    mesh.from_pydata(positions, [], faces)
    mesh.update()
    uv_layer = mesh.uv_layers.new(name="DASHR_Unique")
    for polygon in mesh.polygons:
        polygon.use_smooth = True
        for loop_index in polygon.loop_indices:
            vertex_index = mesh.loops[loop_index].vertex_index
            uv_layer.data[loop_index].uv = vertex_parameters[vertex_index]
    obj = bpy.data.objects.new("DASHR_TorusFixture", mesh)
    bpy.context.collection.objects.link(obj)
    bpy.context.view_layer.objects.active = obj
    obj.select_set(True)
    return obj


def create_sphere_fixture(subdivisions=2):
    if subdivisions < 1 or subdivisions > 5:
        raise ValueError("sphere fixture subdivisions must be in 1..5")
    bpy.ops.mesh.primitive_ico_sphere_add(subdivisions=subdivisions, radius=1.5)
    obj = bpy.context.active_object
    obj.name = "DASHR_SphereFixture"
    obj.data.name = "DASHR_SphereFixtureMesh"
    mesh = obj.data
    mesh.update()
    for polygon in mesh.polygons:
        polygon.use_smooth = True

    uv_layer = mesh.uv_layers.new(name="DASHR_Unique")
    mesh.uv_layers.active = uv_layer
    face_count = len(mesh.polygons)
    columns = math.ceil(math.sqrt(face_count))
    rows = math.ceil(face_count / columns)
    inset = 0.1
    for face_index, polygon in enumerate(mesh.polygons):
        if len(polygon.loop_indices) != 3:
            raise ValueError("sphere fixture must remain triangulated")
        column = face_index % columns
        row = face_index // columns
        u_min = (column + inset) / columns
        u_max = (column + 1.0 - inset) / columns
        v_min = (row + inset) / rows
        v_max = (row + 1.0 - inset) / rows
        triangle_uvs = ((u_min, v_min), (u_max, v_min), (u_min, v_max))
        for loop_index, uv in zip(polygon.loop_indices, triangle_uvs, strict=True):
            uv_layer.data[loop_index].uv = uv

    obj["dashr_fixture"] = "icosphere_per_face_atlas"
    obj["dashr_uv_island_count"] = face_count
    obj["dashr_uv_seam_edge_count"] = face_count * 3 // 2
    bpy.context.view_layer.objects.active = obj
    obj.select_set(True)
    return obj


def active_object(args):
    if args.fixture in ("torus", "sphere"):
        for obj in list(bpy.context.scene.objects):
            if obj.type == "MESH":
                bpy.data.objects.remove(obj, do_unlink=True)
        if args.fixture == "sphere":
            return create_sphere_fixture(args.subdivisions)
        return create_torus_fixture(args.major_segments, args.minor_segments)
    obj = bpy.data.objects.get(args.object_name) if args.object_name else bpy.context.active_object
    if obj is None or obj.type != "MESH":
        raise ValueError("select a mesh object or provide --object <mesh name>")
    return obj


def as_vec3(value):
    return [float(value.x), float(value.y), float(value.z)]


def export_asset(obj):
    if any(modifier.type == "ARMATURE" for modifier in obj.modifiers):
        raise ValueError(
            "static exporter rejects armature modifiers; evaluated-frame and rig export are later modes"
        )
    depsgraph = bpy.context.evaluated_depsgraph_get()
    evaluated_obj = obj.evaluated_get(depsgraph)
    mesh = evaluated_obj.to_mesh(preserve_all_data_layers=True, depsgraph=depsgraph)
    try:
        if not mesh.uv_layers.active:
            raise ValueError("mesh needs one active unique UV atlas")
        uv_data = mesh.uv_layers.active.data
        mesh.calc_loop_triangles()
        triangles = list(mesh.loop_triangles)
        if not triangles:
            raise ValueError("evaluated mesh contains no triangles")
        if any(vertex.groups for vertex in mesh.vertices):
            raise ValueError(
                "static exporter rejects vertex groups; rigged export requires the later skeleton contract"
            )

        world = evaluated_obj.matrix_world
        normal_matrix = world.to_3x3().inverted().transposed()
        loop_normals = getattr(mesh, "corner_normals", None)
        vertices = []
        indices = []
        expected_uv_orientation = None
        topology_edges = Counter()
        edge_face = {}
        position_scale = max(
            1.0,
            max((world @ vertex.co).length for vertex in mesh.vertices),
        )
        position_tolerance = position_scale * 1e-6

        def position_key(position):
            return tuple(round(float(component) / position_tolerance) for component in position)

        for face_id, triangle in enumerate(triangles):
            loop_ids = list(triangle.loops)
            points = [world @ mesh.vertices[mesh.loops[i].vertex_index].co for i in loop_ids]
            uvs = [Vector(uv_data[i].uv) for i in loop_ids]
            edge1 = points[1] - points[0]
            edge2 = points[2] - points[0]
            delta1 = uvs[1] - uvs[0]
            delta2 = uvs[2] - uvs[0]
            determinant = delta1.x * delta2.y - delta1.y * delta2.x
            if abs(determinant) <= 1e-12:
                raise ValueError(f"triangle {face_id} has degenerate UV area")
            orientation = 1 if determinant > 0.0 else -1
            if expected_uv_orientation is None:
                expected_uv_orientation = orientation
            elif orientation != expected_uv_orientation:
                raise ValueError(f"triangle {face_id} has inconsistent UV winding")
            tangent = (edge1 * delta2.y - edge2 * delta1.y) / determinant
            bitangent = (edge2 * delta1.x - edge1 * delta2.x) / determinant

            corner_positions = []
            for corner, loop_index in enumerate(loop_ids):
                loop = mesh.loops[loop_index]
                if loop_normals is not None:
                    local_normal = loop_normals[loop_index].vector
                elif hasattr(loop, "normal"):
                    local_normal = loop.normal
                else:
                    local_normal = mesh.vertices[loop.vertex_index].normal
                normal = (normal_matrix @ local_normal).normalized()
                position = points[corner]
                uv = uvs[corner]
                vertex_index = len(vertices)
                vertices.append(
                    {
                        "position": as_vec3(position),
                        "uv": [float(uv.x), float(uv.y), 1.0],
                        "weights": [1.0, 0.0, 0.0, 0.0],
                        "normal": as_vec3(normal),
                        "tangent": as_vec3(tangent),
                        "bitangent": as_vec3(bitangent),
                    }
                )
                indices.append(vertex_index)
                corner_positions.append(position_key(as_vec3(position)))

            for edge_slot in range(3):
                start = corner_positions[edge_slot]
                end = corner_positions[(edge_slot + 1) % 3]
                key = tuple(sorted((start, end)))
                topology_edges[key] += 1
                edge_face.setdefault(key, face_id)

        bad_edges = [(key, uses, edge_face[key]) for key, uses in topology_edges.items() if uses != 2]
        if bad_edges:
            key, uses, face_id = bad_edges[0]
            raise ValueError(
                f"unsupported open/nonmanifold edge at face {face_id}: geometric edge {key} has {uses} triangle uses"
            )

        asset_document = {
            "schema_version": SCHEMA_VERSION,
            "mesh": {"vertices": vertices, "indices": indices},
        }
        encoded = (json.dumps(asset_document, separators=(",", ":"), sort_keys=True) + "\n").encode()
        manifest = {
            "schema": "dashr.blender-export-manifest.v1",
            "blender_version": bpy.app.version_string,
            "source_object": obj.name,
            "source_mesh": obj.data.name,
            "deformation_mode": "static_evaluated_mesh",
            "coordinate_space": "Blender scene world coordinates",
            "scene_unit_scale": float(bpy.context.scene.unit_settings.scale_length),
            "uv_layer": mesh.uv_layers.active.name,
            "triangulation": "evaluated Blender loop triangles",
            "basis": "per-triangle metric derivative from world position and UV",
            "source_vertices": len(mesh.vertices),
            "triangles": len(triangles),
            "exported_loop_vertices": len(vertices),
            "topology_geometric_edges": len(topology_edges),
            "asset_sha256": hashlib.sha256(encoded).hexdigest(),
        }
        if obj.get("dashr_fixture") == "icosphere_per_face_atlas":
            manifest["fixture"] = obj["dashr_fixture"]
            manifest["uv_island_count"] = int(obj["dashr_uv_island_count"])
            manifest["seam_edge_count"] = int(obj["dashr_uv_seam_edge_count"])
        return encoded, manifest
    finally:
        evaluated_obj.to_mesh_clear()


def main():
    args = cli_args()
    obj = active_object(args)
    encoded, manifest = export_asset(obj)
    args.output_dir.mkdir(parents=True, exist_ok=True)
    (args.output_dir / "asset.json").write_bytes(encoded)
    (args.output_dir / "manifest.json").write_text(
        json.dumps(manifest, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    print(
        "DASHR export:",
        manifest["triangles"],
        "triangles,",
        manifest["exported_loop_vertices"],
        "loop vertices, sha256",
        manifest["asset_sha256"],
    )


if __name__ == "__main__":
    main()
