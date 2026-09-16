"""Build a comic shooting-brake and export it for Todora.

Run:
    /Applications/Blender.app/Contents/MacOS/Blender --background --python tools/make_shooting_brake.py
"""

from __future__ import annotations

import math
from pathlib import Path

import bmesh
import bpy
from mathutils import Matrix, Vector

ROOT = Path(__file__).resolve().parents[1]
GLB = ROOT / "assets" / "models" / "shooting_brake.glb"
BLEND = ROOT / "art" / "models" / "shooting_brake.blend"
PREVIEW = Path("/tmp/todora_shooting_brake.png")
PREVIEW_GAME = Path("/tmp/todora_shooting_brake_game.png")
PREVIEW_SIDE = Path("/tmp/todora_shooting_brake_side.png")

# Blender: +Y forward, +Z up. glTF +Y-up export maps that to Bevy -Z forward.
# Proportions follow a CLA shooting-brake: long roof, short rear, wheels in the arches.
WHEEL_R = 0.20
WHEEL_W = 0.16
TRACK = 0.50
FRONT_Y = 0.70
REAR_Y = -0.76
WELL_R = 0.225
AXIS_X = (0.0, math.radians(90.0), 0.0)


def clear_scene() -> None:
    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.object.delete(use_global=True)
    for block in (bpy.data.meshes, bpy.data.materials, bpy.data.cameras, bpy.data.lights):
        for item in list(block):
            block.remove(item)


def principled(name: str, **inputs) -> bpy.types.Material:
    mat = bpy.data.materials.new(name)
    mat.use_nodes = True
    bsdf = mat.node_tree.nodes.get("Principled BSDF")
    aliases = {
        "base": ["Base Color"],
        "metallic": ["Metallic"],
        "roughness": ["Roughness"],
        "emission": ["Emission Color", "Emission"],
        "emission_strength": ["Emission Strength"],
        "alpha": ["Alpha"],
        "ior": ["IOR"],
        "specular": ["Specular IOR Level", "Specular"],
    }
    for key, value in inputs.items():
        for socket in aliases.get(key, [key]):
            if socket in bsdf.inputs:
                bsdf.inputs[socket].default_value = value
                break
    return mat


def activate(obj: bpy.types.Object) -> None:
    bpy.ops.object.select_all(action="DESELECT")
    obj.select_set(True)
    bpy.context.view_layer.objects.active = obj


def apply_modifier(obj: bpy.types.Object, name: str) -> None:
    activate(obj)
    bpy.ops.object.modifier_apply(modifier=name)


def subdivide(obj: bpy.types.Object, levels: int = 2) -> None:
    mod = obj.modifiers.new("Smooth", "SUBSURF")
    mod.levels = levels
    mod.render_levels = levels
    apply_modifier(obj, mod.name)
    for poly in obj.data.polygons:
        poly.use_smooth = True


def mesh_from_bm(name: str, bm: bmesh.types.BMesh, mat, *, parent, location=(0.0, 0.0, 0.0)) -> bpy.types.Object:
    bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=1e-5)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    mesh = bpy.data.meshes.new(name)
    bm.to_mesh(mesh)
    bm.free()
    for poly in mesh.polygons:
        poly.use_smooth = True
    obj = bpy.data.objects.new(name, mesh)
    obj.location = location
    obj.data.materials.append(mat)
    bpy.context.collection.objects.link(obj)
    obj.parent = parent
    return obj


def add_box(name, size, location, mat, parent, *, bevel=0.02, segments=3, rotation=(0.0, 0.0, 0.0)):
    bm = bmesh.new()
    bmesh.ops.create_cube(bm, size=1.0)
    sx, sy, sz = size
    bmesh.ops.scale(bm, vec=Vector((sx, sy, sz)), verts=bm.verts)
    if bevel > 0.0:
        geom = list(bm.edges)
        bmesh.ops.bevel(
            bm,
            geom=geom,
            offset=min(bevel, min(size) * 0.35),
            segments=segments,
            affect="EDGES",
            profile=0.5,
        )
    obj = mesh_from_bm(name, bm, mat, parent=parent, location=location)
    obj.rotation_euler = rotation
    return obj


def add_cylinder(name, radius, depth, location, mat, parent, *, segments=24, rotation=(0.0, 0.0, 0.0)):
    bm = bmesh.new()
    bmesh.ops.create_cone(
        bm,
        cap_ends=True,
        cap_tris=False,
        segments=segments,
        radius1=radius,
        radius2=radius,
        depth=depth,
    )
    obj = mesh_from_bm(name, bm, mat, parent=parent, location=location)
    obj.rotation_euler = rotation
    return obj


def add_annulus(name, outer_r, inner_r, depth, location, mat, parent, *, segments=32):
    bm = bmesh.new()
    h = depth * 0.5
    outer0, outer1, inner0, inner1 = [], [], [], []
    for k in range(segments):
        a = 2.0 * math.pi * k / segments
        c, s = math.cos(a), math.sin(a)
        outer0.append(bm.verts.new((outer_r * c, outer_r * s, -h)))
        outer1.append(bm.verts.new((outer_r * c, outer_r * s, h)))
        inner0.append(bm.verts.new((inner_r * c, inner_r * s, -h)))
        inner1.append(bm.verts.new((inner_r * c, inner_r * s, h)))
    for k in range(segments):
        n = (k + 1) % segments
        bm.faces.new((outer0[k], outer0[n], outer1[n], outer1[k]))
        bm.faces.new((inner1[k], inner1[n], inner0[n], inner0[k]))
        bm.faces.new((outer0[k], inner0[k], inner0[n], outer0[n]))
        bm.faces.new((outer1[k], outer1[n], inner1[n], inner1[k]))
    obj = mesh_from_bm(name, bm, mat, parent=parent, location=location)
    obj.rotation_euler = AXIS_X
    return obj


def add_prism(name, quad_yz, x0, x1, mat, parent):
    bm = bmesh.new()
    a = [bm.verts.new((x0, y, z)) for y, z in quad_yz]
    b = [bm.verts.new((x1, y, z)) for y, z in quad_yz]
    bm.faces.new(a)
    bm.faces.new(list(reversed(b)))
    for i in range(4):
        bm.faces.new((a[i], a[(i + 1) % 4], b[(i + 1) % 4], b[i]))
    return mesh_from_bm(name, bm, mat, parent=parent)


def rounded_rect(half_w: float, zmin: float, zmax: float, corner: float, n: int = 6) -> list[tuple[float, float]]:
    r = min(corner, half_w * 0.85, (zmax - zmin) * 0.45)
    hw = half_w
    corners = (
        ((hw - r, zmin + r), -math.pi / 2, 0.0),
        ((hw - r, zmax - r), 0.0, math.pi / 2),
        ((-hw + r, zmax - r), math.pi / 2, math.pi),
        ((-hw + r, zmin + r), math.pi, 3 * math.pi / 2),
    )
    pts: list[tuple[float, float]] = []
    for (cx, cz), a0, a1 in corners:
        for k in range(n):
            t = k / n
            a = a0 + (a1 - a0) * t
            pts.append((cx + r * math.cos(a), cz + r * math.sin(a)))
    return pts


def add_loft(name: str, stations: list[tuple], mat, parent) -> bpy.types.Object:
    bm = bmesh.new()
    rings: list[list] = []
    for y, half_w, zmin, zmax, corner in stations:
        ring = [bm.verts.new((x, y, z)) for x, z in rounded_rect(half_w, zmin, zmax, corner)]
        rings.append(ring)
    bm.verts.ensure_lookup_table()
    for i in range(len(rings) - 1):
        a, b = rings[i], rings[i + 1]
        count = len(a)
        for j in range(count):
            bm.faces.new((a[j], a[(j + 1) % count], b[(j + 1) % count], b[j]))
    bm.faces.new(rings[0])
    bm.faces.new(list(reversed(rings[-1])))
    return mesh_from_bm(name, bm, mat, parent=parent)


def add_panel(name: str, quad, thickness: float, mat, parent) -> bpy.types.Object:
    a, b, c, d = (Vector(p) for p in quad)
    normal = (b - a).cross(d - a).normalized()
    bm = bmesh.new()
    outer = [bm.verts.new(p) for p in (a, b, c, d)]
    inner = [bm.verts.new(p - normal * thickness) for p in (a, b, c, d)]
    bm.faces.new(outer)
    bm.faces.new(list(reversed(inner)))
    for i in range(4):
        bm.faces.new((outer[i], outer[(i + 1) % 4], inner[(i + 1) % 4], inner[i]))
    return mesh_from_bm(name, bm, mat, parent=parent)


def add_wheel(tag: str, x: float, y: float, mats: dict, parent) -> None:
    z = WHEEL_R
    outward = 1.0 if x > 0.0 else -1.0
    hub = bpy.data.objects.new(f"Wheel{tag}", None)
    hub.empty_display_size = 0.1
    hub.location = (x, y, z)
    bpy.context.collection.objects.link(hub)
    hub.parent = parent

    lip_r = WHEEL_R * 0.82
    dish_r = WHEEL_R * 0.72
    cap_r = WHEEL_R * 0.22
    face_x = outward * (WHEEL_W * 0.36)
    spoke_x1 = face_x + outward * 0.012

    add_annulus(f"Tire{tag}", WHEEL_R, lip_r * 0.98, WHEEL_W, (0.0, 0.0, 0.0), mats["rubber"], hub, segments=36)
    add_annulus(f"Lip{tag}", lip_r, dish_r, WHEEL_W * 0.22, (face_x - outward * 0.006, 0.0, 0.0), mats["chrome"], hub)
    add_cylinder(
        f"Dish{tag}",
        dish_r,
        0.014,
        (face_x - outward * 0.008, 0.0, 0.0),
        mats["aero"],
        hub,
        segments=36,
        rotation=AXIS_X,
    )

    n_spokes = 12
    inner_r = cap_r * 1.15
    outer_r = dish_r * 0.94
    spoke_width = 0.09
    for i in range(n_spokes):
        a = i * (2.0 * math.pi / n_spokes)
        quad = [
            (inner_r * math.cos(a), inner_r * math.sin(a)),
            (inner_r * math.cos(a + spoke_width), inner_r * math.sin(a + spoke_width)),
            (outer_r * math.cos(a + spoke_width), outer_r * math.sin(a + spoke_width)),
            (outer_r * math.cos(a), outer_r * math.sin(a)),
        ]
        add_prism(f"Spoke{tag}{i}", quad, face_x, spoke_x1, mats["aero"], hub)

    add_cylinder(
        f"Cap{tag}",
        cap_r,
        0.016,
        (face_x + outward * 0.002, 0.0, 0.0),
        mats["aero"],
        hub,
        segments=20,
        rotation=AXIS_X,
    )
    add_annulus(f"CapRing{tag}", cap_r * 1.12, cap_r * 0.55, 0.008, (spoke_x1, 0.0, 0.0), mats["chrome"], hub)


def cut_wheel_wells(body: bpy.types.Object) -> None:
    activate(body)
    for i, (x, y) in enumerate(((-TRACK, FRONT_Y), (TRACK, FRONT_Y), (-TRACK, REAR_Y), (TRACK, REAR_Y))):
        bm = bmesh.new()
        bmesh.ops.create_cone(
            bm,
            cap_ends=True,
            cap_tris=False,
            segments=32,
            radius1=WELL_R,
            radius2=WELL_R,
            depth=0.22,
        )
        bmesh.ops.rotate(
            bm,
            verts=list(bm.verts),
            cent=(0.0, 0.0, 0.0),
            matrix=Matrix.Rotation(math.radians(90.0), 4, "Y"),
        )
        mesh = bpy.data.meshes.new(f"WellCut{i}")
        bm.to_mesh(mesh)
        bm.free()
        cutter = bpy.data.objects.new(f"WellCut{i}", mesh)
        cutter.location = (math.copysign(0.49, x), y, WHEEL_R)
        bpy.context.collection.objects.link(cutter)
        bpy.context.view_layer.update()
        mod = body.modifiers.new(name=f"Well{i}", type="BOOLEAN")
        mod.operation = "DIFFERENCE"
        mod.object = cutter
        if hasattr(mod, "solver"):
            try:
                mod.solver = "EXACT"
            except TypeError:
                pass
        apply_modifier(body, mod.name)
        bpy.data.objects.remove(cutter, do_unlink=True)
        bpy.data.meshes.remove(mesh)
    for poly in body.data.polygons:
        poly.use_smooth = True


def look_at(obj: bpy.types.Object, target: Vector) -> None:
    direction = target - obj.location
    obj.rotation_euler = direction.to_track_quat("-Z", "Y").to_euler()


def build_car(parent, mats: dict) -> None:
    # Lower body stays at beltline height so the hood does not crease into the cabin.
    body = add_loft(
        "Body",
        [
            (1.18, 0.34, 0.06, 0.18, 0.03),
            (1.14, 0.46, 0.06, 0.32, 0.05),
            (1.08, 0.50, 0.07, 0.40, 0.07),
            (0.96, 0.51, 0.08, 0.44, 0.08),
            (0.80, 0.52, 0.09, 0.46, 0.08),
            (0.70, 0.53, 0.09, 0.48, 0.09),
            (0.50, 0.52, 0.09, 0.46, 0.08),
            (0.20, 0.51, 0.09, 0.46, 0.08),
            (-0.12, 0.51, 0.09, 0.46, 0.08),
            (-0.46, 0.52, 0.09, 0.46, 0.08),
            (-0.76, 0.53, 0.09, 0.47, 0.09),
            (-0.90, 0.50, 0.08, 0.42, 0.07),
            (-1.04, 0.46, 0.07, 0.34, 0.06),
            (-1.14, 0.40, 0.06, 0.24, 0.05),
            (-1.20, 0.32, 0.06, 0.16, 0.03),
        ],
        mats["paint"],
        parent,
    )
    subdivide(body, 2)
    cut_wheel_wells(body)

    cabin = add_loft(
        "Cabin",
        [
            (0.22, 0.45, 0.44, 0.48, 0.03),
            (0.08, 0.46, 0.44, 0.66, 0.05),
            (-0.08, 0.47, 0.44, 0.74, 0.06),
            (-0.38, 0.47, 0.44, 0.75, 0.06),
            (-0.62, 0.48, 0.44, 0.74, 0.06),
            (-0.82, 0.48, 0.44, 0.60, 0.05),
            (-0.98, 0.46, 0.44, 0.48, 0.04),
            (-1.04, 0.43, 0.44, 0.45, 0.03),
        ],
        mats["paint"],
        parent,
    )
    subdivide(cabin, 2)

    add_box("Spoiler", (0.68, 0.08, 0.022), (0.0, -0.68, 0.748), mats["paint"], parent, bevel=0.01)

    add_panel(
        "Windshield",
        [(-0.40, 0.16, 0.49), (0.40, 0.16, 0.49), (0.35, -0.04, 0.725), (-0.35, -0.04, 0.725)],
        0.018,
        mats["glass"],
        parent,
    )
    add_panel(
        "RearGlass",
        [(-0.37, -0.66, 0.72), (0.37, -0.66, 0.72), (0.39, -0.96, 0.49), (-0.39, -0.96, 0.49)],
        0.018,
        mats["glass"],
        parent,
    )
    add_panel(
        "SideGlassL",
        [(-0.468, 0.08, 0.495), (-0.458, -0.04, 0.715), (-0.478, -0.62, 0.715), (-0.490, -0.78, 0.495)],
        0.018,
        mats["glass"],
        parent,
    )
    add_panel(
        "SideGlassR",
        [(0.468, 0.08, 0.495), (0.490, -0.78, 0.495), (0.478, -0.62, 0.715), (0.458, -0.04, 0.715)],
        0.018,
        mats["glass"],
        parent,
    )
    add_box("CabinDark", (0.62, 0.58, 0.10), (0.0, -0.32, 0.56), mats["cabin"], parent, bevel=0.03)

    add_box("BeltL", (0.01, 0.88, 0.014), (-0.50, -0.32, 0.485), mats["chrome"], parent, bevel=0.003, segments=1)
    add_box("BeltR", (0.01, 0.88, 0.014), (0.50, -0.32, 0.485), mats["chrome"], parent, bevel=0.003, segments=1)
    add_box("RockerL", (0.008, 1.15, 0.01), (-0.525, -0.04, 0.115), mats["chrome"], parent, bevel=0.003, segments=1)
    add_box("RockerR", (0.008, 1.15, 0.01), (0.525, -0.04, 0.115), mats["chrome"], parent, bevel=0.003, segments=1)

    add_box("HandleFL", (0.03, 0.08, 0.018), (-0.52, 0.02, 0.40), mats["chrome"], parent, bevel=0.006, segments=1)
    add_box("HandleFR", (0.03, 0.08, 0.018), (0.52, 0.02, 0.40), mats["chrome"], parent, bevel=0.006, segments=1)
    add_box("HandleRL", (0.03, 0.08, 0.018), (-0.52, -0.38, 0.40), mats["chrome"], parent, bevel=0.006, segments=1)
    add_box("HandleRR", (0.03, 0.08, 0.018), (0.52, -0.38, 0.40), mats["chrome"], parent, bevel=0.006, segments=1)

    add_box("Grille", (0.42, 0.04, 0.12), (0.0, 1.13, 0.30), mats["trim"], parent, bevel=0.012, segments=2)
    add_box("GrilleChrome", (0.46, 0.02, 0.14), (0.0, 1.12, 0.30), mats["chrome"], parent, bevel=0.008, segments=2)
    add_cylinder("Badge", 0.035, 0.02, (0.0, 1.155, 0.38), mats["chrome"], parent, segments=16, rotation=(math.radians(90), 0, 0))

    add_box("HeadL", (0.18, 0.04, 0.06), (-0.34, 1.115, 0.33), mats["lamp"], parent, bevel=0.018, segments=3)
    add_box("HeadR", (0.18, 0.04, 0.06), (0.34, 1.115, 0.33), mats["lamp"], parent, bevel=0.018, segments=3)
    add_box("TailL", (0.24, 0.02, 0.055), (-0.20, -1.188, 0.36), mats["tail"], parent, bevel=0.008)
    add_box("TailR", (0.24, 0.02, 0.055), (0.20, -1.188, 0.36), mats["tail"], parent, bevel=0.008)

    add_box("MirrorL", (0.10, 0.06, 0.05), (-0.58, 0.14, 0.52), mats["paint"], parent, bevel=0.015)
    add_box("MirrorR", (0.10, 0.06, 0.05), (0.58, 0.14, 0.52), mats["paint"], parent, bevel=0.015)
    add_box("GlassMirrorL", (0.08, 0.016, 0.036), (-0.58, 0.17, 0.52), mats["glass"], parent, bevel=0.006)
    add_box("GlassMirrorR", (0.08, 0.016, 0.036), (0.58, 0.17, 0.52), mats["glass"], parent, bevel=0.006)

    add_box("Plate", (0.28, 0.016, 0.09), (0.0, -1.20, 0.22), mats["plate"], parent, bevel=0.006, segments=1)
    add_cylinder(
        "ExhaustL",
        0.028,
        0.06,
        (-0.28, -1.20, 0.12),
        mats["chrome"],
        parent,
        segments=14,
        rotation=(math.radians(90), 0, 0),
    )
    add_cylinder(
        "ExhaustR",
        0.028,
        0.06,
        (0.28, -1.20, 0.12),
        mats["chrome"],
        parent,
        segments=14,
        rotation=(math.radians(90), 0, 0),
    )

    add_wheel("FL", -TRACK, FRONT_Y, mats, parent)
    add_wheel("FR", TRACK, FRONT_Y, mats, parent)
    add_wheel("RL", -TRACK, REAR_Y, mats, parent)
    add_wheel("RR", TRACK, REAR_Y, mats, parent)


def setup_studio() -> None:
    world = bpy.context.scene.world or bpy.data.worlds.new("World")
    bpy.context.scene.world = world
    world.use_nodes = True
    bg = world.node_tree.nodes.get("Background")
    if bg:
        bg.inputs[0].default_value = (0.62, 0.66, 0.70, 1.0)
        bg.inputs[1].default_value = 0.9

    ground = add_box(
        "StudioGround",
        (10.0, 10.0, 0.04),
        (0.0, 0.0, -0.02),
        principled("Ground", base=(0.35, 0.35, 0.36, 1.0), roughness=0.85, metallic=0.0),
        parent=None,
        bevel=0.0,
    )
    ground.parent = None

    sun = bpy.data.lights.new("Sun", "SUN")
    sun.energy = 8.0
    sun.angle = 0.15
    sun_obj = bpy.data.objects.new("Sun", sun)
    sun_obj.rotation_euler = (math.radians(55), math.radians(20), math.radians(40))
    bpy.context.collection.objects.link(sun_obj)

    fill = bpy.data.lights.new("Fill", "AREA")
    fill.energy = 280.0
    fill.size = 5.0
    fill_obj = bpy.data.objects.new("Fill", fill)
    fill_obj.location = (-3.2, -1.5, 2.6)
    look_at(fill_obj, Vector((0.0, -0.2, 0.35)))
    bpy.context.collection.objects.link(fill_obj)

    cam_data = bpy.data.cameras.new("PreviewCam")
    cam_data.lens = 50
    cam = bpy.data.objects.new("PreviewCam", cam_data)
    cam.location = (-3.1, -3.6, 1.25)
    look_at(cam, Vector((0.0, -0.15, 0.36)))
    bpy.context.collection.objects.link(cam)
    bpy.context.scene.camera = cam

    side_data = bpy.data.cameras.new("SideCam")
    side_data.lens = 55
    side = bpy.data.objects.new("SideCam", side_data)
    side.location = (-4.2, -0.05, 0.85)
    look_at(side, Vector((0.0, -0.05, 0.34)))
    bpy.context.collection.objects.link(side)

    game_cam_data = bpy.data.cameras.new("GameCam")
    game_cam_data.lens = 35
    game_cam = bpy.data.objects.new("GameCam", game_cam_data)
    game_cam.location = (0.0, -16.0, 18.0)
    look_at(game_cam, Vector((0.0, 0.0, 0.4)))
    bpy.context.collection.objects.link(game_cam)


def export_car(root: bpy.types.Object) -> None:
    bpy.ops.object.select_all(action="DESELECT")
    root.select_set(True)
    for obj in root.children_recursive:
        obj.select_set(True)
    bpy.context.view_layer.objects.active = root
    bpy.ops.export_scene.gltf(
        filepath=str(GLB),
        export_format="GLB",
        use_selection=True,
        export_apply=True,
        export_cameras=False,
        export_lights=False,
        export_yup=True,
        export_animations=False,
        export_extras=False,
    )


def render_preview() -> None:
    scene = bpy.context.scene
    scene.render.engine = "BLENDER_EEVEE"
    scene.render.resolution_x = 1280
    scene.render.resolution_y = 720
    scene.render.image_settings.file_format = "PNG"
    scene.render.film_transparent = False
    scene.render.filepath = str(PREVIEW)
    bpy.ops.render.render(write_still=True)
    scene.camera = bpy.data.objects["SideCam"]
    scene.render.filepath = str(PREVIEW_SIDE)
    bpy.ops.render.render(write_still=True)
    scene.camera = bpy.data.objects["GameCam"]
    scene.render.filepath = str(PREVIEW_GAME)
    bpy.ops.render.render(write_still=True)
    scene.camera = bpy.data.objects["PreviewCam"]


def main() -> None:
    GLB.parent.mkdir(parents=True, exist_ok=True)
    BLEND.parent.mkdir(parents=True, exist_ok=True)
    clear_scene()
    bpy.context.scene.unit_settings.system = "METRIC"

    root = bpy.data.objects.new("ShootingBrake", None)
    bpy.context.collection.objects.link(root)

    mats = {
        "paint": principled("Paint", base=(0.02, 0.02, 0.022, 1.0), metallic=0.32, roughness=0.22),
        "glass": principled("Glass", base=(0.12, 0.16, 0.20, 1.0), metallic=0.4, roughness=0.06),
        "rubber": principled("Rubber", base=(0.03, 0.03, 0.03, 1.0), metallic=0.0, roughness=0.92),
        "aero": principled("Aero", base=(0.04, 0.04, 0.045, 1.0), metallic=0.4, roughness=0.38),
        "chrome": principled("Chrome", base=(0.82, 0.83, 0.85, 1.0), metallic=1.0, roughness=0.12),
        "trim": principled("Trim", base=(0.06, 0.06, 0.07, 1.0), metallic=0.35, roughness=0.4),
        "lamp": principled(
            "Lamp",
            base=(1.0, 0.94, 0.78, 1.0),
            metallic=0.0,
            roughness=0.18,
            emission=(1.0, 0.94, 0.78, 1.0),
            emission_strength=6.0,
        ),
        "tail": principled(
            "Tail",
            base=(0.75, 0.06, 0.06, 1.0),
            metallic=0.1,
            roughness=0.22,
            emission=(0.9, 0.02, 0.02, 1.0),
            emission_strength=3.0,
        ),
        "cabin": principled("Cabin", base=(0.05, 0.05, 0.06, 1.0), metallic=0.0, roughness=0.85),
        "plate": principled("Plate", base=(0.92, 0.92, 0.94, 1.0), metallic=0.0, roughness=0.55),
    }

    build_car(root, mats)
    setup_studio()
    export_car(root)
    render_preview()

    bpy.context.preferences.filepaths.save_version = 0
    bpy.ops.wm.save_as_mainfile(filepath=str(BLEND))
    print(f"Wrote {GLB}")
    print(f"Wrote {BLEND}")
    print(f"Wrote {PREVIEW}")


if __name__ == "__main__":
    main()
