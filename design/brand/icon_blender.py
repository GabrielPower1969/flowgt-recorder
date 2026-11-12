"""App icon candidate, rendered with Blender: a small reel-to-reel front panel.

    blender -b -P icon_blender.py -- <output dir> [size] [samples] [light|dark] [full|small]

Everything is built from primitives in this script: no downloaded models, textures or HDRIs.
Writes icon-<variant>[-small]-<size>.png (transparent background) into the output directory.
"small" is the drawing for 16 to 48 pixels: the plate, two larger reels and three larger keys, nothing else.
"""
import math
import os
import sys

import bmesh
import bpy

argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
OUT = argv[0] if argv else os.getcwd()
SIZE = int(argv[1]) if len(argv) > 1 else 1024
SAMPLES = int(argv[2]) if len(argv) > 2 else 160
DARK = len(argv) > 3 and argv[3] == "dark"
SMALL = len(argv) > 4 and argv[4] == "small"

bpy.ops.wm.read_factory_settings(use_empty=True)
scene = bpy.context.scene


def srgb(hex_colour, alpha=1.0):
    """Hex sRGB to the linear RGBA Blender expects."""
    h = hex_colour.lstrip("#")
    out = []
    for i in (0, 2, 4):
        c = int(h[i:i + 2], 16) / 255
        out.append(c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4)
    return (*out, alpha)


def set_input(node, names, value):
    """Principled BSDF socket names changed between Blender versions; use the first that exists."""
    for name in names:
        if name in node.inputs:
            node.inputs[name].default_value = value
            return True
    return False


def material(name, colour, metallic=0.0, roughness=0.5, transmission=0.0, ior=1.45, coat=0.0, aniso=0.0, alpha=1.0):
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    bsdf = next(n for n in m.node_tree.nodes if n.type == "BSDF_PRINCIPLED")
    set_input(bsdf, ["Base Color"], srgb(colour))
    set_input(bsdf, ["Metallic"], metallic)
    set_input(bsdf, ["Roughness"], roughness)
    set_input(bsdf, ["IOR"], ior)
    set_input(bsdf, ["Transmission Weight", "Transmission"], transmission)
    set_input(bsdf, ["Coat Weight", "Clearcoat"], coat)
    set_input(bsdf, ["Anisotropic"], aniso)
    set_input(bsdf, ["Alpha"], alpha)
    return m


ALU = material("aluminium", "#4A4D50" if DARK else "#D5D7D8", metallic=1.0, roughness=0.34, aniso=0.6)


def brush(m, lo, hi, bump):
    """Fine lines along X in the roughness and a trace of relief: brushed metal, built from noise only."""
    tree = m.node_tree
    bsdf = next(n for n in tree.nodes if n.type == "BSDF_PRINCIPLED")
    coords = tree.nodes.new("ShaderNodeTexCoord")
    mapping = tree.nodes.new("ShaderNodeMapping")
    mapping.inputs["Scale"].default_value = (0.6, 220.0, 1.0)
    noise = tree.nodes.new("ShaderNodeTexNoise")
    noise.inputs["Scale"].default_value = 6.0
    noise.inputs["Detail"].default_value = 3.0
    spread = tree.nodes.new("ShaderNodeMapRange")
    spread.inputs[1].default_value = 0.3
    spread.inputs[2].default_value = 0.7
    spread.inputs[3].default_value = lo
    spread.inputs[4].default_value = hi
    relief = tree.nodes.new("ShaderNodeBump")
    relief.inputs["Strength"].default_value = bump
    tree.links.new(coords.outputs["Object"], mapping.inputs["Vector"])
    tree.links.new(mapping.outputs[0], noise.inputs["Vector"])
    tree.links.new(noise.outputs[0], spread.inputs[0])
    tree.links.new(spread.outputs[0], bsdf.inputs["Roughness"])
    tree.links.new(noise.outputs[0], relief.inputs["Height"])
    tree.links.new(relief.outputs[0], bsdf.inputs["Normal"])


brush(ALU, 0.24, 0.46, 0.035)
ALU_DARK = material("well", "#17181A" if DARK else "#5E6163", metallic=0.6, roughness=0.5)
KEY = material("key", "#F1F0EA", roughness=0.38, coat=0.25)
RED = material("record", "#E2452D", roughness=0.35, coat=0.4)
TAPE = material("tape", "#7A5240" if DARK else "#3A2A24", roughness=0.42, coat=0.15)
HUB = material("hub", "#F4F3EE", roughness=0.45)
# Clear plastic as a thin milky film: the tape shows through, the surface still takes a highlight.
ACRYLIC = material("flange", "#FFFFFF", roughness=0.08, coat=1.0, alpha=0.30)
RIM = material("flange-rim", "#FFFFFF", roughness=0.15, coat=0.6, alpha=0.85)
GLASS = material("glass", "#16181A", roughness=0.12, coat=0.6)
STEEL = material("steel", "#B4B7B9", metallic=1.0, roughness=0.25)


def finish(obj, mat, bevel=0.0, segments=4, smooth=True):
    obj.data.materials.append(mat)
    if bevel > 0:
        mod = obj.modifiers.new("bevel", "BEVEL")
        mod.width = bevel
        mod.segments = segments
        mod.limit_method = "ANGLE"
    return obj


def box(name, size, location, mat, bevel=0.0, segments=4):
    bpy.ops.mesh.primitive_cube_add(size=1, location=location)
    obj = bpy.context.active_object
    obj.name = name
    obj.scale = size
    bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
    return finish(obj, mat, bevel, segments)


def disc(name, radius, depth, location, mat, bevel=0.0, vertices=128):
    bpy.ops.mesh.primitive_cylinder_add(radius=radius, depth=depth, vertices=vertices, location=location)
    obj = bpy.context.active_object
    obj.name = name
    return finish(obj, mat, bevel, 3)


def cut(target, cutter):
    mod = target.modifiers.new("cut", "BOOLEAN")
    mod.operation = "DIFFERENCE"
    mod.object = cutter
    cutter.hide_render = True
    cutter.hide_viewport = True


# The plate: a rounded square, 2 units across, lying in the XY plane and seen from +Z.
plate = box("plate", (2.0, 2.0, 0.16), (0, 0, -0.08), ALU, bevel=0.0)
# Round the four corners in plan: bevel only the edges that run along Z. Then soften the rim.
bm = bmesh.new()
bm.from_mesh(plate.data)
upright = [e for e in bm.edges if abs(e.verts[0].co.x - e.verts[1].co.x) < 1e-6 and abs(e.verts[0].co.y - e.verts[1].co.y) < 1e-6]
bmesh.ops.bevel(bm, geom=upright, offset=0.42, segments=32, profile=0.5, affect="EDGES")
bm.to_mesh(plate.data)
bm.free()
rim = plate.modifiers.new("rim", "BEVEL")
rim.width = 0.03
rim.segments = 5
rim.limit_method = "ANGLE"
rim.angle_limit = math.radians(60)


def polar(r, deg):
    a = math.radians(deg)
    return (r * math.cos(a), r * math.sin(a))


def quad(p0, p1, p2, steps=6):
    """Points along a quadratic Bezier, without its start."""
    out = []
    for i in range(1, steps + 1):
        t = i / steps
        out.append(((1 - t) ** 2 * p0[0] + 2 * (1 - t) * t * p1[0] + t * t * p2[0], (1 - t) ** 2 * p0[1] + 2 * (1 - t) * t * p1[1] + t * t * p2[1]))
    return out


def window_outline(r1, r2, mid, half, c):
    """One window of the flange: a slice of a ring with rounded corners. Same shape as the app's SVG reel."""
    a0, a1 = mid - half, mid + half
    d1, d2 = math.degrees(c / r1), math.degrees(c / r2)
    pts = [polar(r1 + c, a0), polar(r2 - c, a0)]
    pts += quad(polar(r2 - c, a0), polar(r2, a0), polar(r2, a0 + d2))
    pts += [polar(r2, a0 + d2 + (a1 - a0 - 2 * d2) * i / 28) for i in range(1, 29)]
    pts += quad(polar(r2, a1 - d2), polar(r2, a1), polar(r2 - c, a1))
    pts.append(polar(r1 + c, a1))
    pts += quad(polar(r1 + c, a1), polar(r1, a1), polar(r1, a1 - d1))
    pts += [polar(r1, a1 - d1 - (a1 - a0 - 2 * d1) * i / 14) for i in range(1, 15)]
    pts += quad(polar(r1, a0 + d1), polar(r1, a0), polar(r1 + c, a0))
    return pts[:-1]


def flange(x, y, z, r):
    """The clear disc with its three windows, as a filled 2D curve with holes, given a little thickness."""
    curve = bpy.data.curves.new("flange", "CURVE")
    curve.dimensions = "2D"
    curve.fill_mode = "BOTH"
    curve.extrude = 0.010
    k = r / 186
    outlines = [[polar(r, 360 * i / 160) for i in range(160)]]
    outlines += [window_outline(74 * k, 166 * k, mid, 42, 14 * k) for mid in (30, 150, 270)]
    for outline in outlines:
        spline = curve.splines.new("POLY")
        spline.points.add(len(outline) - 1)
        for point, (px, py) in zip(spline.points, outline):
            point.co = (px, py, 0, 1)
        spline.use_cyclic_u = True
    curve.materials.append(ACRYLIC)
    obj = bpy.data.objects.new("flange", curve)
    obj.location = (x, y, z)
    scene.collection.objects.link(obj)
    return obj


def reel(x, y, r, tape):
    tape_radius = r * tape / 186
    if tape_radius > 0:
        disc("tape", tape_radius, 0.07, (x, y, 0.035), TAPE, bevel=0.006)
    flange(x, y, 0.085, r)
    # A thin bright ring on the edge of the flange, so the reel reads as a circle on any background.
    bpy.ops.mesh.primitive_torus_add(major_radius=r - 0.006, minor_radius=0.016 if SMALL else 0.007, major_segments=128, minor_segments=12, location=(x, y, 0.096))
    ring = bpy.context.active_object
    ring.name = "rim"
    ring.data.materials.append(RIM)
    disc("hub", r * 60 / 186, 0.105, (x, y, 0.0525), HUB, bevel=0.008)
    disc("spindle", 0.034, 0.15, (x, y, 0.075), STEEL, bevel=0.006)


REEL_X, REEL_Y, REEL_R = (0.49, 0.30, 0.47) if SMALL else (0.485, 0.38, 0.45)
reel(-REEL_X, REEL_Y, REEL_R, 150)
reel(REEL_X, REEL_Y, REEL_R, 96 if SMALL else 84)

# The read-out between the reels and the keys: a dark window with the tape running to it.
if not SMALL:
    box("monitor", (0.46, 0.25, 0.05), (0, -0.235, 0.025), GLASS, bevel=0.012)

# The tape itself: off the bottom of each reel, over a guide roller, past the top of the monitor.
def strand(p, q):
    length = math.hypot(q[0] - p[0], q[1] - p[1])
    obj = box("tape-run", (length, 0.007, 0.03), ((p[0] + q[0]) / 2, (p[1] + q[1]) / 2, 0.032), TAPE)
    obj.rotation_euler[2] = math.atan2(q[1] - p[1], q[0] - p[0])


def lower_tangent(cx, cy, r, px, py):
    """Where a line from (px, py) touches the circle, on its lower side."""
    base = math.atan2(py - cy, px - cx)
    swing = math.acos(r / math.hypot(px - cx, py - cy))
    a, b = [(cx + r * math.cos(base + s * swing), cy + r * math.sin(base + s * swing)) for s in (1, -1)]
    return a if a[1] < b[1] else b


for side, pack in (() if SMALL else ((-1, 0.45 * 150 / 186), (1, 0.45 * 84 / 186))):
    roller = (side * 0.275, -0.085)
    strand(lower_tangent(side * 0.485, 0.38, pack, roller[0], roller[1] + 0.018), (roller[0], roller[1] + 0.018))
    disc("roller", 0.02, 0.07, (roller[0], roller[1], 0.035), STEEL, bevel=0.004, vertices=48)
if not SMALL:
    strand((-0.275, -0.067), (0.275, -0.067))

# The keys in their well; the red dot is on the middle one.
KEY_Y, KEY_STEP, KEY_SIZE, DOT_R = (-0.56, 0.47, 0.42, 0.125) if SMALL else (-0.62, 0.31, 0.285, 0.062)
box("well", (2 * KEY_STEP + KEY_SIZE + 0.075, KEY_SIZE + 0.055, 0.03), (0, KEY_Y, 0.005), ALU_DARK, bevel=0.012)
for i, kx in enumerate((-KEY_STEP, 0.0, KEY_STEP)):
    box(f"key-{i}", (KEY_SIZE, KEY_SIZE, 0.075), (kx, KEY_Y, 0.05), KEY, bevel=0.016)
disc("dot", DOT_R, 0.006, (0, KEY_Y, 0.0905), RED)

# Screws in the corners of the plate.
for sx in (() if SMALL else (-0.78, 0.78)):
    for sy in (-0.78, 0.78):
        disc("screw", 0.028, 0.012, (sx, sy, 0.004), STEEL, bevel=0.003, vertices=32)

# Light: a large soft key from the upper left, a fill, and a strip that draws the highlight on the metal.
def area(name, location, rotation, size, power, colour=(1, 1, 1)):
    light = bpy.data.lights.new(name, "AREA")
    light.size = size
    light.energy = power
    light.color = colour
    obj = bpy.data.objects.new(name, light)
    obj.location = location
    obj.rotation_euler = [math.radians(v) for v in rotation]
    scene.collection.objects.link(obj)


area("key", (-2.2, 2.4, 4.0), (-28, -24, 0), 4.0, 175)
area("fill", (2.6, -1.6, 3.0), (24, 30, 0), 3.0, 30, (0.96, 0.98, 1.0))
area("strip", (0.0, 3.2, 1.6), (-62, 0, 0), 2.6, 60)

world = bpy.data.worlds.new("world")
world.use_nodes = True
background = next(n for n in world.node_tree.nodes if n.type == "BACKGROUND")
background.inputs[0].default_value = (0.55, 0.56, 0.58, 1)
background.inputs[1].default_value = 0.55
scene.world = world

# Straight on, orthographic, with a little room around the plate for its shadowless edge.
camera_data = bpy.data.cameras.new("camera")
camera_data.type = "ORTHO"
camera_data.ortho_scale = 2.14
camera = bpy.data.objects.new("camera", camera_data)
camera.location = (0, 0, 6)
scene.collection.objects.link(camera)
scene.camera = camera

scene.render.engine = "CYCLES"
scene.cycles.samples = SAMPLES
scene.cycles.use_denoising = True
scene.cycles.max_bounces = 10
scene.cycles.transmission_bounces = 10
scene.render.film_transparent = True
scene.render.resolution_x = SIZE
scene.render.resolution_y = SIZE
scene.render.resolution_percentage = 100
scene.render.image_settings.file_format = "PNG"
scene.render.image_settings.color_mode = "RGBA"
try:
    scene.view_settings.view_transform = "Standard"
except TypeError:
    pass
VARIANT = ("dark" if DARK else "light") + ("-small" if SMALL else "")
scene.render.filepath = os.path.join(OUT, f"icon-{VARIANT}-{SIZE}.png")

# Bake modifiers into the meshes, drop the boolean cutters, then smooth only the curved parts:
# flat faces stay flat (a smooth-shaded cylinder cap looks like a dome).
depsgraph = bpy.context.evaluated_depsgraph_get()
for obj in [o for o in scene.objects if o.type == "CURVE"]:
    mesh_obj = bpy.data.objects.new(obj.name, bpy.data.meshes.new_from_object(obj.evaluated_get(depsgraph)))
    mesh_obj.matrix_world = obj.matrix_world
    scene.collection.objects.link(mesh_obj)
    bpy.data.objects.remove(obj, do_unlink=True)
for obj in [o for o in scene.objects if o.type == "MESH" and not o.hide_render]:
    baked = bpy.data.meshes.new_from_object(obj.evaluated_get(depsgraph))
    obj.modifiers.clear()
    obj.data = baked
for obj in [o for o in scene.objects if o.type == "MESH" and o.hide_render]:
    bpy.data.objects.remove(obj, do_unlink=True)
for obj in [o for o in scene.objects if o.type == "MESH"]:
    for poly in obj.data.polygons:
        poly.use_smooth = True
    obj.data.set_sharp_from_angle(angle=math.radians(32))

os.makedirs(OUT, exist_ok=True)
bpy.ops.wm.save_as_mainfile(filepath=os.path.join(OUT, f"icon-{VARIANT}.blend"))
bpy.ops.render.render(write_still=True)
print("rendered", scene.render.filepath)
