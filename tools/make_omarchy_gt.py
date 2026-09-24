"""Build the Omarchy GT #95, Todora's own endurance racer.

Original Blender geometry, fitted to Todora's existing wheel-contact coordinates:
a front-engined GT of no particular maker, in the pale blue and orange
DHH raced in, OMARCHY RACING on the wing and DHH on the plate. No real
maker's or sponsor's name or badge is on it. Notes: docs/models/omarchy-gt-95.md.
Run with Blender --background --python tools/make_omarchy_gt.py.
"""
from __future__ import annotations

import math
import sys
from pathlib import Path

import bmesh
import bpy
from mathutils import Matrix, Vector

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parent))
import make_shooting_brake as mesh

ROOT = Path(__file__).resolve().parents[1]
GLB = ROOT / 'assets/models/omarchy_gt_95.glb'
BLEND = ROOT / 'art/models/omarchy_gt_95.blend'
PREVIEW = ROOT / 'art/models/omarchy_gt_95.png'
COCKPIT_GLB = ROOT / 'assets/models/omarchy_gt_95_cockpit.glb'

# The glasshouse, kept as a mesh of its own so the cockpit view can hide it
# and look out past the bonnet from inside.
CABIN_PARTS = ('Fastback cabin', 'Front windscreen', 'Rear windscreen', 'Window seal', 'Side glazing',
               'Accent window surround', 'Window black seal', 'B pillar', 'Accent sunstrip',
               'Windscreen banner', 'Windscreen wiper', 'Roof identity', 'Danish', 'Roof aerial', 'Roof camera')


def surface(name, vertices, faces, material, parent, smooth=False):
    data = bpy.data.meshes.new(name)
    data.from_pydata(vertices, [], faces)
    data.materials.append(material)
    data.update()
    obj = bpy.data.objects.new(name, data)
    bpy.context.collection.objects.link(obj)
    obj.parent = parent
    for face in data.polygons:
        face.use_smooth = smooth
    return obj


def panel(name, points, material, parent):
    return surface(name, points, [tuple(range(len(points)))], material, parent)


def tube(name, points, radius, material, parent):
    curve = bpy.data.curves.new(name, 'CURVE')
    curve.dimensions = '3D'
    curve.resolution_u = 2
    curve.bevel_depth = radius
    curve.bevel_resolution = 2
    spline = curve.splines.new('POLY')
    spline.points.add(len(points) - 1)
    for p, xyz in zip(spline.points, points):
        p.co = (*xyz, 1)
    obj = bpy.data.objects.new(name, curve)
    bpy.context.collection.objects.link(obj)
    obj.parent = parent
    obj.data.materials.append(material)
    mesh.activate(obj)
    bpy.ops.object.convert(target='MESH')
    return obj


def text(name, body, center, size, material, parent, *, right=(1, 0, 0), up=(0, 0, 1)):
    """Mesh lettering, with its own surface frame: no external fonts or textures."""
    curve = bpy.data.curves.new(name, 'FONT')
    curve.body = body
    curve.size = size
    curve.align_x = 'CENTER'
    curve.align_y = 'CENTER'
    curve.resolution_u = 3
    curve.extrude = 0
    obj = bpy.data.objects.new(name, curve)
    bpy.context.collection.objects.link(obj)
    obj.parent = parent
    r, u = Vector(right).normalized(), Vector(up).normalized()
    obj.rotation_euler = Matrix((r, u, r.cross(u))).transposed().to_euler()
    obj.location = center
    obj.data.materials.append(material)
    mesh.activate(obj)
    bpy.ops.object.convert(target='MESH')
    return obj


def disc(name, center, radius, material, parent, right=(1, 0, 0), up=(0, 0, 1)):
    c, r, u = Vector(center), Vector(right), Vector(up)
    points = [c + radius * (r * math.cos(a * math.tau / 48) + u * math.sin(a * math.tau / 48)) for a in range(48)]
    return panel(name, points, material, parent)


def bodywork(root, m):
    # A GT of its own: a wedge nose, a flat waist and a squared-off Kamm tail.
    sections = [
        (1.25, .36, .075, .228, .030),
        (1.22, .455, .080, .325, .045),
        (1.14, .508, .090, .381, .055),
        (1.02, .537, .095, .420, .055),
        (.84, .548, .095, .455, .060),
        (.70, .548, .095, .470, .055),
        (.51, .532, .095, .442, .050),
        (.30, .510, .095, .415, .055),
        (.04, .503, .095, .410, .055),
        (-.22, .510, .095, .423, .052),
        (-.44, .536, .095, .443, .055),
        (-.63, .548, .095, .468, .050),
        (-.76, .552, .095, .474, .052),
        (-.92, .550, .095, .468, .048),
        (-1.06, .540, .095, .455, .038),
        (-1.16, .525, .095, .436, .028),
        (-1.21, .505, .095, .412, .020),
    ]
    body = mesh.add_loft('GT sculpted body', sections, m['paint'], root)
    mesh.subdivide(body, 2)
    mesh.cut_wheel_wells(body)
    # Keep body strips continuous at the contact patches; separate thin arch lips.
    for tag, x, y in [('FL', -.50, .70), ('FR', .50, .70), ('RL', -.50, -.76), ('RR', .50, -.76)]:
        side = math.copysign(1, x)
        arc = [(side * .550, y + .230 * math.cos(a), .20 + .230 * math.sin(a)) for a in [math.pi * k / 36 for k in range(37)]]
        tube('Wheel arch ' + tag, arc, .009, m['paint'], root)
    cabin = mesh.add_loft('Fastback cabin', [
        (.34, .405, .380, .416, .027),
        (.23, .427, .389, .490, .035),
        (.06, .421, .392, .638, .050),
        (-.09, .406, .392, .694, .060),
        (-.30, .411, .395, .710, .060),
        (-.47, .423, .403, .688, .065),
        (-.67, .449, .414, .592, .055),
        (-.86, .467, .410, .485, .040),
        (-.97, .470, .403, .431, .020),
    ], m['paint'], root)
    mesh.subdivide(cabin, 2)

    # Glass is a curved grid, rather than a flat plane floating off the coupe.
    for rear in [False, True]:
        rows = [(.260, .421, .391), (.150, .523, .377), (.030, .638, .356), (-.015, .668, .344)] if not rear else [(-.490, .675, .362), (-.600, .623, .381), (-.730, .547, .402), (-.865, .474, .420)]
        points = []
        for y, z, width in rows:
            for i in range(13):
                t = (i / 12) * 2 - 1
                points.append((t * width, y, z + .014 * (1 - t*t)))
        faces = [(j*13+i, j*13+i+1, (j+1)*13+i+1, (j+1)*13+i) for j in range(3) for i in range(12)]
        surface('Rear windscreen' if rear else 'Front windscreen', points, faces, m['glass'], root, True)
        outline = points[:13] + [points[j*13+12] for j in range(1,4)] + list(reversed(points[39:51])) + [points[j*13] for j in range(2,-1,-1)]
        tube('Window seal', outline, .008, m['black'], root)

    for side in [-1, 1]:
        side_points = [(side*.424,.205,.456),(side*.391,-.032,.657),(side*.398,-.365,.670),(side*.431,-.639,.544),(side*.456,-.757,.469),(side*.447,-.391,.448)]
        panel('Side glazing', side_points, m['glass'], root)
        tube('Accent window surround', side_points + side_points[:1], .010, m['accent'], root)
        tube('Window black seal', [(x+side*.003,y,z) for x,y,z in side_points+side_points[:1]], .005, m['black'], root)
        tube('B pillar', [(side*.404,-.403,.641),(side*.448,-.435,.457)], .012, m['black'], root)
        # Lower door shut line, side sill and front fender air outlet.
        tube('Door shut', [(side*.512,.266,.403),(side*.519,.285,.177),(side*.514,.15,.149),(side*.523,-.34,.149),(side*.536,-.439,.215),(side*.537,-.43,.414)], .0018, m['black'], root)
        mesh.add_box('Side skirt', (.047,1.055,.090),(side*.523,-.03,.102),m['carbon'],root,bevel=.01)
        tube('Accent sill pinstripe', [(side*.55,.455,.148),(side*.55,-.48,.148)], .006, m['accent'], root)
        mesh.add_box('Door handle', (.010,.082,.017),(side*.512,-.237,.395),m['silver'],root,bevel=.006)
        # Three upright gills behind each front wheel.
        for y in [.47,.43,.39]:
            mesh.add_box('Fender gill', (.012,.018,.070),(side*.532,y,.345),m['black'],root,bevel=.006)
        tube('Mirror stalk',[(side*.427,.170,.447),(side*.564,.156,.487)],.009,m['carbon'],root)
        mirror=mesh.add_box('Carbon mirror',(.117,.077,.052),(side*.585,.145,.504),m['carbon'],root,bevel=.022,segments=5)
        mesh.add_box('Mirror glass',(.083,.009,.032),(side*.585,.103,.504),m['glass'],root,bevel=.012)
        mesh.add_cylinder('Side exhaust',.031,.050,(side*.558,-.190,.109),m['silver'],root,rotation=mesh.AXIS_X)
        disc('Exhaust opening',(side*.586,-.190,.109),.023,m['black'],root,right=(0,1,0))
        # Fuel filler on the rear quarter.
        disc('Filler surround',(side*.551,-.509,.411),.032,m['black'],root,right=(0,1,0))
        disc('Fuel cap',(side*.552,-.509,.411),.020,m['silver'],root,right=(0,1,0))

    # Tapered bonnet stripe conforms to the sculpted surface.
    rows=[(1.221,.340,.106),(1.140,.391,.109),(1.02,.427,.112),(.84,.457,.114),(.70,.472,.115),(.51,.444,.114),(.30,.417,.108)]
    pts=[(side*w,y,z+.004) for y,z,w in rows for side in [-1,1]]
    surface('Accent bonnet stripe',pts,[(2*i,2*i+1,2*i+3,2*i+2) for i in range(len(rows)-1)],m['accent'],root)
    for side in [-1,1]:
        tube('Bonnet stripe keyline',[(side*(w+.004),y,z+.004) for y,z,w in rows],.003,m['navy'],root)
        # Recessed cooling slots and bonnet catches.
        for y,z in [(.60,.455),(.68,.469),(.76,.467)]:
            panel('Bonnet vent',[(side*.254+dx,y+dy,z) for dx,dy in [(-.055,-.02),(.055,-.02),(.055,.02),(-.055,.02)]],m['black'],root)
        for x,y,z in [(side*.340,.981,.428),(side*.280,.342,.421)]:
            disc('Bonnet fastener',(x,y,z+.003),.010,m['silver'],root,up=(0,1,0))

    # Top windscreen sunstrip and a single endurance wiper.
    panel('Accent sunstrip',[(-.346,.009,.668),(.346,.009,.668),(.360,.059,.624),(-.360,.059,.624)],m['accent'],root)
    text('Windscreen banner','OMARCHY',(0,.087,.622),.037,m['white'],root,right=(-1,0,0),up=(0,-.67,.74))
    tube('Windscreen wiper',[(.21,.247,.444),(.045,.151,.555),(-.115,.100,.597)],.004,m['carbon'],root)
    tube('Roof aerial',[(.045,-.345,.709),(.045,-.377,.950)],.0025,m['black'],root)
    mesh.add_box('Roof camera',(.039,.050,.022),(-.061,-.304,.708),m['carbon'],root,bevel=.005)


def nose_and_tail(root,m):
    # Twin hexagonal intakes either side of a body-colour spine.
    for side in [-1,1]:
        rim=[(side*.045,1.268,.310),(side*.045,1.276,.200),(side*.110,1.281,.176),(side*.360,1.271,.176),(side*.392,1.258,.250),(side*.330,1.241,.318)]
        panel('Grille dark opening',rim,m['black'],root)
        tube('Accent intake rim',rim+rim[:1],.010,m['accent'],root)
        for k in range(8):
            x=side*(.085+k*.038)
            tube('Grille mesh vertical',[(x,1.279,.190),(x,1.266,.300)],.0015,m['mesh'],root)
        for z in [.215,.245,.275]:
            tube('Grille mesh horizontal',[(side*.06,1.278,z),(side*.36,1.268,z)],.0015,m['mesh'],root)
        # Two round endurance lamps set into each intake.
        for x in [side*.155,side*.270]:
            mesh.add_cylinder('Endurance light housing',.034,.026,(x,1.289,.242),m['carbon'],root,segments=32,rotation=(math.pi/2,0,0))
            disc('Yellow endurance lens',(x,1.304,.242),.028,m['yellow'],root)
    # Slim blade headlamps along the top corners of the nose.
    for side in [-1,1]:
        outline=[(side*.300,1.170,.354),(side*.478,1.070,.392),(side*.482,1.040,.404),(side*.300,1.140,.368)]
        panel('Headlamp carbon bucket',outline,m['black'],root)
        tube('Headlight LED',[(side*.310,1.160,.360),(side*.474,1.062,.398)],.005,m['white'],root)
    splitter=[(-.57,1.135,.065),(-.49,1.292,.065),(-.28,1.332,.064),(.28,1.332,.064),(.49,1.292,.065),(.57,1.135,.065)]
    panel('Carbon front splitter',splitter,m['carbon'],root)
    tube('Splitter leading edge',splitter,.012,m['carbon'],root)
    mesh.add_box('Lower radiator',(.662,.025,.041),(0,1.256,.117),m['black'],root,bevel=.011)
    for side in [-1,1]:
        for z in [.155,.198]:
            panel('Front dive plane',[(side*.485,1.164,z),(side*.587,1.112,z+.018),(side*.548,.962,z-.018)],m['carbon'],root)
    # A plain roundel on the nose: the car's number, no maker's badge.
    disc('Nose roundel',(0,1.200,.372),.034,m['accent'],root,up=(0,-.35,.94))
    text('Nose number','95',(0,1.200,.375),.030,m['white'],root,right=(-1,0,0),up=(0,-.35,.94))

    # A full-width light bar across the tail, the valance and the diffuser.
    tube('Tail light bar',[(-.47,-1.196,.352),(-.25,-1.214,.356),(.25,-1.214,.356),(.47,-1.196,.352)],.010,m['red'],root)
    for side in [-1,1]:
        mesh.add_box('Tail lamp end',(.060,.010,.040),(side*.455,-1.198,.352),m['red'],root,bevel=.006)
    mesh.add_box('Rear mesh valance',(.765,.028,.103),(0,-1.214,.207),m['black'],root,bevel=.02)
    for x in [-.34,-.25,-.16,.16,.25,.34]:
        tube('Rear grille slot',[(x,-1.232,.175),(x,-1.232,.239)],.002,m['mesh'],root)
    panel('Diffuser tray',[(-.47,-.99,.059),(.47,-.99,.059),(.49,-1.271,.130),(-.49,-1.271,.130)],m['carbon'],root)
    for x in [-.44,-.29,-.14,0,.14,.29,.44]:
        panel('Diffuser strake',[(x,-.96,.062),(x,-1.270,.137),(x,-1.266,.052),(x,-1.07,.038)],m['carbon'],root)
    tube('Rear accent pinstripe',[(-.48,-1.181,.139),(0,-1.243,.140),(.48,-1.181,.139)],.006,m['accent'],root)
    mesh.add_box('Rear rain light',(.055,.014,.029),(0,-1.252,.166),m['red'],root,bevel=.004)
    # The number plate, in the middle of the valance.
    mesh.add_box('Number plate',(.200,.008,.058),(0,-1.236,.214),m['white'],root,bevel=.004)
    text('Number plate text','DHH',(0,-1.241,.214),.042,m['black'],root,right=(1,0,0))
    # Two mounts and a cambered aerofoil across the rear.
    for x in [-.293,.293]:
        panel('Wing upright',[(x,-.963,.418),(x,-1.114,.440),(x,-1.116,.720),(x,-1.043,.714)],m['carbon'],root)
        mesh.add_box('Wing foot',(.078,.166,.016),(x,-1.019,.445),m['carbon'],root,bevel=.006)
    wing=[]
    for x in [-.632,.632]:
        for y,z in [(-.984,.735),(-1.020,.751),(-1.164,.748),(-1.226,.731),(-1.217,.721),(-1.036,.730)]:
            wing.append((x,y,z))
    faces=[tuple(range(5,-1,-1)),tuple(range(6,12))]+[(i,(i+1)%6,(i+1)%6+6,i+6) for i in range(6)]
    surface('Rear wing aerofoil',wing,faces,m['carbon'],root,True)
    for side in [-1,1]:
        panel('Wing endplate',[(side*.636,-.960,.657),(side*.636,-.960,.785),(side*.636,-1.239,.785),(side*.636,-1.254,.658)],m['carbon'],root)
        text('Wing endplate name','TODORA',(side*.638,-1.101,.722),.030,m['white'],root,right=(0,side,0))
    text('Wing top lettering','OMARCHY RACING',(0,-1.103,.753),.063,m['white'],root,right=(1,0,0),up=(0,1,0))
    text('Rear lettering','OMARCHY',(0,-1.214,.300),.034,m['silver'],root,right=(1,0,0))


def wheels(root,m):
    # Reuse the exact wheel hub coordinates read by Rust's physics and skid marks.
    for tag,x,y in [('FL',-.50,.70),('FR',.50,.70),('RL',-.50,-.76),('RR',.50,-.76)]:
        side=math.copysign(1,x)
        hub=bpy.data.objects.new('Wheel'+tag,None)
        bpy.context.collection.objects.link(hub)
        hub.parent=root
        hub.location=(x,y,.20)
        mesh.add_annulus('Slick '+tag,.20,.150,.16,(0,0,0),m['rubber'],hub,segments=64)
        # Rounded shoulders on the slick, fully within the physics' .20 m radius.
        for outward in [-1,1]:
            mesh.add_annulus('Tyre shoulder '+tag,.197,.147,.012,(outward*.077,0,0),m['rubber'],hub,segments=64)
        face=side*.083
        mesh.add_annulus('Rim '+tag,.155,.135,.012,(face,0,0),m['silver'],hub,segments=48)
        mesh.add_cylinder('Brake disc '+tag,.131,.012,(side*.025,0,0),m['brake'],hub,segments=48,rotation=mesh.AXIS_X)
        # Calipers stay with the chassis; only the rotor and wheel spin.
        mesh.add_box('Brake caliper '+tag,(.035,.035,.084),(x+side*.044,y+.104,.20),m['yellow'],root,bevel=.008)
        for k in range(10):
            a=k*math.tau/10
            quad=[(.029*math.cos(a-.15),.029*math.sin(a-.15)),(.136*math.cos(a+.015),.136*math.sin(a+.015)),(.136*math.cos(a+.115),.136*math.sin(a+.115)),(.029*math.cos(a+.25),.029*math.sin(a+.25))]
            mesh.add_prism('Forged spoke '+tag,quad,face-side*.011,face,m['silver'],hub)
        mesh.add_cylinder('Centerlock '+tag,.026,.024,(face,0,0),m['gunmetal'],hub,segments=12,rotation=mesh.AXIS_X)
        for a in range(16):
            theta=a*math.tau/16
            disc('Brake drill '+tag,(side*.037,.111*math.cos(theta),.111*math.sin(theta)),.004,m['black'],hub,right=(0,1,0))
        for yoff,zoff in [(0,.173),(0,-.174)]:
            text('Tyre lettering '+tag,'TODORA',(side*.085,yoff,zoff),.018,m['white'],hub,right=(0,side,0))


def livery(root,m):
    for side in [-1,1]:
        # Number board: white, with an orange field, a navy header and a white 95.
        x=side*.540
        panel('Number board',[(x,.242,.163),(x,-.015,.163),(x,-.015,.402),(x,.242,.402)],m['white'],root)
        panel('Number field',[(x+side*.001,.228,.182),(x+side*.001,.000,.182),(x+side*.001,.000,.347),(x+side*.001,.228,.347)],m['accent'],root)
        panel('Number header',[(x+side*.002,.231,.350),(x+side*.002,-.002,.350),(x+side*.002,-.002,.376),(x+side*.002,.231,.376)],m['navy'],root)
        text('Door number','95',(x+side*.004,.114,.262),.176,m['white'],root,right=(0,side,0))
        text('Number header text','TODORA',(x+side*.004,.114,.362),.018,m['white'],root,right=(0,side,0))
        text('Door name','OMARCHY',(side*.550,-.215,.330),.052,m['navy'],root,right=(0,side,0))
        text('Door strapline','RACING',(side*.550,-.215,.275),.026,m['accent'],root,right=(0,side,0))
        text('Sill lettering','TODORA',(side*.551,.364,.107),.024,m['navy'],root,right=(0,side,0))
    text('Bonnet 95','95',(0,.464,.453),.118,m['white'],root,right=(-1,0,0),up=(0,-1,0))
    text('Roof identity','95',(0,-.255,.714),.143,m['navy'],root,up=(0,1,0))
    # Small Danish flag at the roof's rear edge.
    panel('Danish flag',[(.13,-.41,.694),(.24,-.41,.694),(.24,-.46,.678),(.13,-.46,.678)],m['red'],root)
    panel('Danish cross vertical',[(.165,-.41,.695),(.174,-.41,.695),(.174,-.46,.679),(.165,-.46,.679)],m['white'],root)
    panel('Danish cross horizontal',[(.13,-.431,.688),(.24,-.431,.688),(.24,-.440,.685),(.13,-.440,.685)],m['white'],root)


def fit_details(root):
    """Lay glass and decals onto the finished, subdivided body surfaces."""
    body = bpy.data.objects['GT sculpted body']
    cabin = bpy.data.objects['Fastback cabin']
    bpy.context.view_layer.update()
    for obj in list(root.children_recursive):
        if obj.type != 'MESH':
            continue
        name = obj.name
        direction = Vector((0, 0, -1))
        target = None
        offset = .003
        if name.startswith(('Front windscreen', 'Rear windscreen', 'Window seal', 'Accent sunstrip', 'Windscreen banner', 'Windscreen wiper', 'Roof identity', 'Danish')):
            target = cabin
            if name.startswith(('Window seal', 'Danish cross')):
                offset = .005
            elif name.startswith('Accent sunstrip'):
                offset = .007
            elif name.startswith('Windscreen banner'):
                offset = .010
        elif name.startswith(('Side glazing', 'Accent window surround', 'Window black seal', 'B pillar')):
            target = cabin
            direction = Vector((-1 if obj.data.vertices[0].co.x > 0 else 1, 0, 0))
            if name.startswith('Accent window surround'):
                offset = .006
            elif name.startswith(('Window black seal', 'B pillar')):
                offset = .009
        elif name.startswith(('Accent bonnet stripe', 'Bonnet stripe', 'Bonnet 95', 'Bonnet vent', 'Nose roundel', 'Nose number', 'Headlamp', 'Headlight')):
            target = body
            if name.startswith(('Nose roundel', 'Headlamp carbon')):
                offset = .005
            elif name.startswith(('Bonnet 95', 'Nose number', 'Headlight')):
                offset = .008
        elif name.startswith(('Number board', 'Number field', 'Number header', 'Door number', 'Door name', 'Door strapline', 'Sill lettering')):
            target = body
            center = obj.matrix_world @ obj.data.vertices[0].co
            direction = Vector((-1 if center.x > 0 else 1, 0, 0))
            if name.startswith('Number field') or (name.startswith('Number header') and not name.startswith('Number header text')):
                offset = .005
            elif name.startswith(('Door number', 'Number header text', 'Door name', 'Door strapline', 'Sill lettering')):
                offset = .007
        if target is None:
            continue
        # A decal must follow the surface between its corners too. Subdivide
        # broad patches before projection so the curved body cannot poke through.
        if name.startswith(('Front windscreen', 'Rear windscreen', 'Side glazing', 'Accent bonnet stripe', 'Accent sunstrip', 'Number board', 'Number field', 'Number header', 'Nose roundel', 'Headlamp carbon')):
            bm = bmesh.new()
            bm.from_mesh(obj.data)
            bmesh.ops.triangulate(bm, faces=list(bm.faces))
            bmesh.ops.subdivide_edges(bm, edges=list(bm.edges), cuts=7, use_grid_fill=True)
            bm.to_mesh(obj.data)
            bm.free()
        transform = obj.matrix_world.copy()
        inverse = transform.inverted()
        for vertex in obj.data.vertices:
            point = transform @ vertex.co
            origin = point - direction * 2
            hit, location, normal, _ = target.ray_cast(origin, direction, distance=4)
            if hit:
                vertex.co = inverse @ (location - direction * offset)
        obj.data.update()


def join(parts, name):
    """Join `parts` into one mesh called `name`, its transform baked in."""
    bpy.ops.object.select_all(action='DESELECT')
    for obj in parts:
        obj.select_set(True)
    bpy.context.view_layer.objects.active = parts[0]
    bpy.ops.object.join()
    obj = bpy.context.object
    obj.name = name
    obj.data.name = name
    # Joining retains the active object's transform; bake it into the mesh,
    # leaving all four wheel origins exactly at the physics contact points.
    bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
    return obj


def join_render_parts(root):
    """One mesh for the body, one for the glasshouse, one per rotating wheel."""
    hubs = [obj for obj in root.children if obj.name in ['WheelFL', 'WheelFR', 'WheelRL', 'WheelRR']]
    parts = [obj for obj in root.children if obj.type == 'MESH']
    join([obj for obj in parts if obj.name.startswith(CABIN_PARTS)], 'GT cabin')
    join([obj for obj in root.children if obj.type == 'MESH' and obj.name != 'GT cabin'], 'GT bodywork')
    for hub in hubs:
        join([obj for obj in hub.children if obj.type == 'MESH'], 'Racing slick and rim ' + hub.name[-2:])


def cockpit(m):
    """The inside, seen from the driver's seat on the left: a carbon tub and
    dashboard with a display and shift lights, the steering wheel (turned by
    the game about its column, the empty's local Z in glTF), A-pillars, a roof
    liner and a roll cage. Only the cockpit camera shows it."""
    root = bpy.data.objects.new('Omarchy GT 95 cockpit', None)
    bpy.context.collection.objects.link(root)
    x = -.19  # the driver's centre line
    panel('Tub floor', [(-.40, .27, .432), (.40, .27, .432), (.40, -.62, .432), (-.40, -.62, .432)], m['inside'], root)
    for side in [-1, 1]:
        mesh.add_box('Door sill', (.035, .86, .05), (side*.385, -.17, .455), m['inside'], root, bevel=.008)
        panel('Door card', [(side*.402, .24, .432), (side*.402, -.50, .432), (side*.402, -.50, .478), (side*.402, .24, .478)], m['alcantara'], root)
    # Dashboard, the cowl over the display, and the display with its shift lights.
    mesh.add_box('Dashboard', (.80, .13, .07), (0, .205, .447), m['inside'], root, bevel=.02)
    mesh.add_box('Instrument cowl', (.24, .09, .035), (x, .16, .497), m['inside'], root, bevel=.012)
    mesh.add_box('Dash display', (.15, .006, .052), (x, .118, .478), m['screen'], root, bevel=.004)
    for k, colour in enumerate(['accent', 'accent', 'accent', 'shift_red', 'shift_red']):
        mesh.add_box('Shift light', (.014, .006, .009), (x - .05 + k*.025, .117, .509), m[colour], root, bevel=.002)
    mesh.add_box('Centre console', (.12, .30, .05), (0, .05, .455), m['inside'], root, bevel=.015)
    for k in range(4):
        mesh.add_box('Console switch', (.016, .016, .006), (-.03 + (k % 2)*.06, .12 - (k // 2)*.05, .482), m['accent' if k == 0 else 'black'], root, bevel=.003)
    tube('Steering column', [(x, .16, .468), (x, .085, .488)], .010, m['black'], root)
    # A-pillars along the windscreen's edges, the header over it, a mirror.
    for side in [-1, 1]:
        tube('A pillar', [(side*.392, .262, .423), (side*.378, .150, .530), (side*.357, .030, .640), (side*.346, -.017, .668)], .018, m['black'], root)
    mesh.add_box('Windscreen header', (.72, .06, .03), (0, -.035, .676), m['inside'], root, bevel=.01)
    panel('Roof liner', [(-.36, -.04, .694), (.36, -.04, .694), (.36, -.48, .694), (-.36, -.48, .694)], m['alcantara'], root)
    tube('Mirror stem', [(0, -.035, .662), (0, -.02, .640)], .004, m['black'], root)
    mesh.add_box('Rear-view mirror', (.11, .012, .032), (0, -.012, .628), m['black'], root, bevel=.008)
    mesh.add_box('Mirror glass', (.098, .002, .024), (0, -.005, .628), m['glass'], root, bevel=.004)
    # The roll cage: along the A-pillars and the roof, the main hoop behind the
    # seats, a bar across the dash and door bars.
    for side in [-1, 1]:
        tube('Cage A bar', [(side*.36, .22, .438), (side*.345, .10, .56), (side*.325, -.02, .652), (side*.32, -.46, .668)], .007, m['cage'], root)
        tube('Cage hoop leg', [(side*.345, -.46, .438), (side*.32, -.46, .668)], .012, m['cage'], root)
        tube('Cage door bar', [(side*.375, .20, .452), (side*.365, -.44, .47)], .010, m['cage'], root)
    tube('Cage hoop top', [(-.32, -.46, .668), (.32, -.46, .668)], .012, m['cage'], root)
    tube('Cage dash bar', [(-.36, .22, .438), (.36, .22, .438)], .010, m['cage'], root)
    tube('Cage diagonal', [(-.32, -.46, .668), (.345, -.46, .438)], .010, m['cage'], root)
    tube('Cage roof bar', [(-.325, -.02, .652), (.32, -.46, .668)], .009, m['cage'], root)
    # The steering wheel: a flat-bottomed rim, three spokes and a hub with a
    # small screen, all in the empty's XZ plane facing the driver.
    wheel = bpy.data.objects.new('SteeringWheel', None)
    bpy.context.collection.objects.link(wheel)
    wheel.parent = root
    wheel.location = (x, .085, .488)
    rim = []
    for k in range(41):
        a = math.tau * k / 40
        rx, rz = .062 * math.cos(a), .058 * math.sin(a)
        rim.append((rx, 0, max(rz, -.042)))
    tube('Wheel rim', rim, .009, m['alcantara'], wheel)
    for end in [(-.060, 0, .0), (.060, 0, .0), (0, 0, -.042)]:
        tube('Wheel spoke', [(0, 0, 0), end], .008, m['inside'], wheel)
    mesh.add_box('Wheel hub', (.062, .02, .042), (0, .004, -.004), m['inside'], wheel, bevel=.008)
    mesh.add_box('Wheel screen', (.045, .003, .022), (0, -.007, .004), m['screen'], wheel, bevel=.002)
    for k in range(3):
        mesh.add_box('Wheel light', (.008, .003, .005), (-.012 + k*.012, -.007, .021), m['accent'], wheel, bevel=.001)
    for side in [-1, 1]:
        mesh.add_box('Wheel button', (.010, .004, .010), (side*.024, -.012, -.015), m['accent' if side < 0 else 'shift_red'], wheel, bevel=.003)
        mesh.add_box('Shift paddle', (.026, .006, .040), (side*.052, .016, .010), m['inside'], wheel, bevel=.004)
    for obj in root.children_recursive:
        if obj.type == 'MESH':
            for mat in obj.data.materials:
                if mat is not None:
                    mat.use_backface_culling = False
    join([obj for obj in root.children if obj.type == 'MESH'], 'Cockpit')
    join([obj for obj in wheel.children if obj.type == 'MESH'], 'Steering wheel')
    return root


def studio():
    scene=bpy.context.scene
    scene.world=bpy.data.worlds.new('Studio world')
    scene.world.use_nodes=True
    bg=scene.world.node_tree.nodes.get('Background')
    bg.inputs[0].default_value=(.12,.15,.19,1)
    bg.inputs[1].default_value=.6
    mesh.add_box('Studio floor',(200,200,.04),(0,0,-.025),mesh.principled('Studio floor',base=(.032,.043,.057,1),roughness=.65),None,bevel=0)
    for name,location,energy,size in [('Key',(-3,2.8,4),480,4),('Fill',(3,1,2.5),320,3),('Rim',(1,-3,3.5),650,3)]:
        light=bpy.data.lights.new(name,'AREA');light.energy=energy;light.shape='DISK';light.size=size
        obj=bpy.data.objects.new(name,light);bpy.context.collection.objects.link(obj);obj.location=location
        mesh.look_at(obj,Vector((0,0,.3)))
    for name,location,target,lens in [('Front',(-3.3,4.5,2.0),(0,0,.32),58),('Rear',(3.3,-4.4,1.9),(0,-.1,.34),58),('Side',(-4.8,0,.95),(0,0,.35),58)]:
        data=bpy.data.cameras.new(name);data.lens=lens
        obj=bpy.data.objects.new(name,data);bpy.context.collection.objects.link(obj);obj.location=location
        mesh.look_at(obj,Vector(target))
    scene.camera=bpy.data.objects['Front']
    scene.render.engine='CYCLES'
    scene.cycles.samples=48
    scene.cycles.use_denoising=True
    scene.render.resolution_x=1440;scene.render.resolution_y=900;scene.render.resolution_percentage=100
    scene.render.image_settings.file_format='PNG'
    scene.view_settings.view_transform='AgX'


def main():
    mesh.clear_scene()
    bpy.context.scene.unit_settings.system='METRIC'
    root=bpy.data.objects.new('Omarchy GT 95',None)
    bpy.context.collection.objects.link(root)
    m={
        # Pale blue. The game repaints the body per car; this is the studio's.
        'paint':mesh.principled('Paint',base=(.448,.674,.750,1),metallic=.28,roughness=.26),
        'accent':mesh.principled('Livery orange',base=(1,.135,.008,1),roughness=.33),
        'navy':mesh.principled('Livery navy',base=(.008,.020,.055,1),roughness=.4),
        'white':mesh.principled('Lettering white',base=(.91,.94,.95,1),roughness=.4),
        'black':mesh.principled('Recess black',base=(.004,.006,.008,1),roughness=.65),
        'carbon':mesh.principled('Carbon aero',base=(.018,.024,.030,1),metallic=.20,roughness=.33),
        'glass':mesh.principled('Smoked glass',base=(.028,.062,.10,1),metallic=.4,roughness=.14),
        'rubber':mesh.principled('Racing slick',base=(.016,.019,.023,1),roughness=.82),
        'silver':mesh.principled('Forged aluminium',base=(.56,.60,.63,1),metallic=.9,roughness=.25),
        'gunmetal':mesh.principled('Centerlock metal',base=(.14,.15,.13,1),metallic=.75,roughness=.28),
        'brake':mesh.principled('Brake rotor',base=(.19,.19,.17,1),metallic=.72,roughness=.6),
        'mesh':mesh.principled('Grille wire',base=(.05,.055,.060,1),metallic=.65,roughness=.48),
        'yellow':mesh.principled('Endurance yellow',base=(1,.73,.015,1),roughness=.19,emission=(1,.65,.006,1),emission_strength=.6),
        'amber':mesh.principled('Amber',base=(1,.22,.015,1),roughness=.3),
        'red':mesh.principled('Tail red',base=(.65,.007,.012,1),roughness=.25,emission=(1,.004,.006,1),emission_strength=.5),
    }
    bodywork(root,m);nose_and_tail(root,m);wheels(root,m);livery(root,m)
    # The same local origins and hub names are the car's animation contract.
    for obj in root.children_recursive:
        if obj.type=='MESH':
            obj.data.update()
            # glTF backface culling must not hide thin aero surfaces underneath.
            for i, mat in enumerate(obj.data.materials):
                if mat is None:
                    obj.data.materials[i] = m['carbon']
                else:
                    mat.use_backface_culling=False
    fit_details(root)
    join_render_parts(root)
    GLB.parent.mkdir(parents=True,exist_ok=True)
    mesh.GLB=GLB
    mesh.export_car(root)
    m['alcantara']=mesh.principled('Alcantara',base=(.003,.003,.004,1),roughness=.95)
    m['cage']=mesh.principled('Roll cage',base=(.10,.11,.12,1),metallic=.5,roughness=.4)
    m['inside']=mesh.principled('Cockpit carbon',base=(.004,.005,.006,1),metallic=.2,roughness=.45)
    m['screen']=mesh.principled('Dash screen',base=(.004,.006,.008,1),roughness=.2,emission=(.02,.05,.03,1),emission_strength=1)
    m['shift_red']=mesh.principled('Shift red',base=(.8,.02,.02,1),roughness=.3,emission=(1,.02,.02,1),emission_strength=1)
    inside=cockpit(m)
    mesh.GLB=COCKPIT_GLB
    mesh.export_car(inside)
    # The studio renders are of the outside.
    for obj in [inside,*inside.children_recursive]:
        obj.hide_render=True
    studio()
    bpy.context.preferences.filepaths.save_version=0
    bpy.ops.wm.save_as_mainfile(filepath=str(BLEND))
    for name,path in [('Front',PREVIEW),('Rear',Path('/tmp/todora-omarchy-rear.png')),('Side',Path('/tmp/todora-omarchy-side.png'))]:
        bpy.context.scene.camera=bpy.data.objects[name]
        bpy.context.scene.render.filepath=str(path)
        bpy.ops.render.render(write_still=True)
    print(f'Wrote {BLEND}, {GLB}, {COCKPIT_GLB} and {PREVIEW}')


if __name__=='__main__':
    main()
