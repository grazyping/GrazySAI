"""Тестовые 3D-модели GrazySAI, сделанные с нуля (в комплект программы не входят).

    blender -b --factory-startup --python tools/3d/make_test_models.py -- OUT_DIR [треугольников]

figure.glb — фигура из простых тел на скелете (позвоночник, голова, руки с пятью пальцами по три
сустава, ноги), жёсткая привязка, морф «Belly», клетчатая текстура; figure.fbx — она же в FBX;
figure.obj — она же без скелета;
scene.glb (и scene.gltf + scene.bin) — несколько предметов с вложенностью; heavy.glb — много треугольников (по умолчанию 1 млн)
для проверки скорости.
"""
import math
import sys

import bmesh
import bpy
from mathutils import Matrix, Vector

argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
OUT = argv[0] if argv else "."
HEAVY = int(argv[1]) if len(argv) > 1 else 1_000_000


def reset():
    bpy.ops.wm.read_factory_settings(use_empty=True)


def checker_material(name):
    img = bpy.data.images.new(name + "_tex", 64, 64)
    img.generated_type = "COLOR_GRID"
    img.pack()
    mat = bpy.data.materials.new(name)
    nt = mat.node_tree
    bsdf = nt.nodes.get("Principled BSDF")
    tex = nt.nodes.new("ShaderNodeTexImage")
    tex.image = img
    nt.links.new(tex.outputs["Color"], bsdf.inputs["Base Color"])
    return mat


# кость: имя, родитель, начало, конец, толщина части тела
def bones():
    b = [
        ("hips", None, (0, 0, 1.0), (0, 0, 1.15), 0.16),
        ("spine", "hips", (0, 0, 1.15), (0, 0, 1.45), 0.15),
        ("neck", "spine", (0, 0, 1.45), (0, 0, 1.55), 0.05),
        ("head", "neck", (0, 0, 1.55), (0, 0, 1.78), 0.1),
    ]
    for side, s in (("L", 1), ("R", -1)):
        b += [
            (f"upper_arm.{side}", "spine", (0.18 * s, 0, 1.42), (0.45 * s, 0, 1.42), 0.045),
            (f"forearm.{side}", f"upper_arm.{side}", (0.45 * s, 0, 1.42), (0.7 * s, 0, 1.42), 0.038),
            (f"hand.{side}", f"forearm.{side}", (0.7 * s, 0, 1.42), (0.78 * s, 0, 1.42), 0.03),
            (f"thigh.{side}", "hips", (0.09 * s, 0, 1.0), (0.1 * s, 0, 0.53), 0.07),
            (f"shin.{side}", f"thigh.{side}", (0.1 * s, 0, 0.53), (0.1 * s, 0, 0.08), 0.05),
            (f"foot.{side}", f"shin.{side}", (0.1 * s, 0, 0.08), (0.1 * s, -0.16, 0.02), 0.04),
        ]
        for f, name in enumerate(("thumb", "index", "middle", "ring", "pinky")):
            y = (f - 2) * 0.018
            x0, z = 0.78 * s, 1.42
            parent = f"hand.{side}"
            for j in range(3):
                n = f"{name}.{j + 1}.{side}"
                b.append((n, parent, (x0 + 0.025 * j * s, y, z), (x0 + 0.025 * (j + 1) * s, y, z), 0.008))
                parent = n
    return b


def figure():
    reset()
    arm_data = bpy.data.armatures.new("Skeleton")
    arm = bpy.data.objects.new("Skeleton", arm_data)
    bpy.context.scene.collection.objects.link(arm)
    bpy.context.view_layer.objects.active = arm
    bpy.ops.object.mode_set(mode="EDIT")
    for name, parent, head, tail, _ in bones():
        e = arm_data.edit_bones.new(name)
        e.head, e.tail = head, tail
        if parent:
            e.parent = arm_data.edit_bones[parent]
            e.use_connect = (Vector(e.parent.tail) - Vector(head)).length < 1e-6
    bpy.ops.object.mode_set(mode="OBJECT")
    # тело: по цилиндру на кость, вершины привязаны к своей кости целиком
    bm = bmesh.new()
    groups = []
    for name, _, head, tail, r in bones():
        h, t = Vector(head), Vector(tail)
        d = t - h
        before = len(bm.verts)
        bmesh.ops.create_cone(bm, cap_ends=True, segments=12, radius1=r, radius2=r * 0.85, depth=d.length)
        m = Matrix.Translation((h + t) / 2) @ Vector((0, 0, 1)).rotation_difference(d.normalized()).to_matrix().to_4x4()
        bm.verts.ensure_lookup_table()
        new = [bm.verts[i] for i in range(before, len(bm.verts))]
        bmesh.ops.transform(bm, matrix=m, verts=new)
        groups.append((name, list(range(before, len(bm.verts)))))
    mesh = bpy.data.meshes.new("Body")
    bm.to_mesh(mesh)
    bm.free()
    body = bpy.data.objects.new("Body", mesh)
    bpy.context.scene.collection.objects.link(body)
    for name, idx in groups:
        body.vertex_groups.new(name=name).add(idx, 1.0, "REPLACE")
    body.parent = arm
    body.modifiers.new("Skeleton", "ARMATURE").object = arm
    # развёртка для текстуры — по высоте и углу
    uv = mesh.uv_layers.new(name="UVMap")
    for loop in mesh.loops:
        co = mesh.vertices[loop.vertex_index].co
        uv.data[loop.index].uv = ((math.atan2(co.y, co.x) / math.tau) % 1.0, co.z / 1.8)
    mesh.materials.append(checker_material("Skin"))
    # морф: живот вперёд
    body.shape_key_add(name="Basis")
    belly = body.shape_key_add(name="Belly")
    belly.value = 0.0
    for name, idx in groups:
        if name in ("hips", "spine"):
            for i in idx:
                if mesh.vertices[i].co.y < 0:
                    belly.data[i].co.y -= 0.06
    bpy.ops.export_scene.gltf(filepath=f"{OUT}/figure.glb", export_format="GLB", export_morph=True, export_skins=True, export_animations=False)
    # OBJ — без скелета и морфов
    bpy.ops.wm.obj_export(filepath=f"{OUT}/figure.obj", export_materials=True)
    # FBX — со скелетом, морфом и текстурой внутри файла (созданную картинку FBX встраивает,
    # только если у неё есть файл)
    for img in bpy.data.images:
        img.filepath_raw = f"{OUT}/{img.name}.png"
        img.file_format = "PNG"
        img.save()
        img.pack()
    bpy.ops.export_scene.fbx(filepath=f"{OUT}/figure.fbx", add_leaf_bones=False, bake_anim=False, path_mode="COPY", embed_textures=True)


def scene():
    reset()
    root = bpy.data.objects.new("Street", None)
    bpy.context.scene.collection.objects.link(root)
    for i in range(4):
        bm = bmesh.new()
        bmesh.ops.create_cube(bm, size=1.0)
        me = bpy.data.meshes.new(f"House{i}")
        bm.to_mesh(me)
        bm.free()
        o = bpy.data.objects.new(f"House{i}", me)
        o.location = (i * 2.5, 0, 0.5 + i * 0.25)
        o.scale = (1, 1, 1 + i * 0.5)
        o.parent = root
        bpy.context.scene.collection.objects.link(o)
        bm = bmesh.new()
        bmesh.ops.create_cone(bm, cap_ends=True, segments=4, radius1=0.8, radius2=0.0, depth=0.6)
        me = bpy.data.meshes.new(f"Roof{i}")
        bm.to_mesh(me)
        bm.free()
        r = bpy.data.objects.new(f"Roof{i}", me)
        r.location = (0, 0, 0.8)
        r.parent = o
        bpy.context.scene.collection.objects.link(r)
    bpy.ops.export_scene.gltf(filepath=f"{OUT}/scene.glb", export_format="GLB")
    # то же отдельными файлами: scene.gltf + scene.bin
    bpy.ops.export_scene.gltf(filepath=f"{OUT}/scene.gltf", export_format="GLTF_SEPARATE")


def heavy():
    reset()
    # сфера с N/2 сегментами по кругу и N по высоте ≈ 2·u·v треугольников; несколько штук сеткой
    per = 8
    tris = max(HEAVY // per, 1000)
    u = int(math.sqrt(tris / 2))
    for k in range(per):
        bm = bmesh.new()
        bmesh.ops.create_uvsphere(bm, u_segments=u, v_segments=u, radius=0.5)
        me = bpy.data.meshes.new(f"Ball{k}")
        bm.to_mesh(me)
        bm.free()
        o = bpy.data.objects.new(f"Ball{k}", me)
        o.location = (k % 4 * 1.2, k // 4 * 1.2, 0)
        bpy.context.scene.collection.objects.link(o)
    bpy.ops.export_scene.gltf(filepath=f"{OUT}/heavy.glb", export_format="GLB", export_materials="NONE")


figure()
scene()
heavy()
print("models ready:", OUT)
