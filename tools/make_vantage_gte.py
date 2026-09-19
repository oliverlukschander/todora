"""Build the 2014 Le Mans GTE Am-winning Aston Martin V8 Vantage #95.

Original Blender geometry, fitted to Todora's existing wheel-contact coordinates.
References and reproduction notes: docs/models/vantage-gte-95.md.
Run with Blender --background --python tools/make_vantage_gte.py.
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
GLB = ROOT / 'assets/models/vantage_gte_95.glb'
BLEND = ROOT / 'art/models/vantage_gte_95.blend'
PREVIEW = ROOT / 'art/models/vantage_gte_95.png'


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
    # Actual coupe silhouette: long low bonnet, swept glasshouse, muscular haunches.
    sections = [
        (1.25, .38, .075, .245, .040),
        (1.22, .47, .080, .340, .055),
        (1.14, .515, .090, .390, .065),
        (1.02, .540, .095, .425, .060),
        (.84, .548, .095, .455, .060),
        (.70, .548, .095, .470, .055),
        (.51, .532, .095, .442, .050),
        (.30, .510, .095, .415, .055),
        (.04, .503, .095, .410, .055),
        (-.22, .510, .095, .423, .052),
        (-.44, .536, .095, .443, .055),
        (-.63, .556, .095, .475, .055),
        (-.76, .558, .095, .480, .060),
        (-.92, .550, .095, .453, .060),
        (-1.06, .525, .095, .420, .045),
        (-1.16, .497, .095, .380, .040),
        (-1.21, .462, .095, .330, .032),
    ]
    body = mesh.add_loft('Vantage sculpted body', sections, m['paint'], root)
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
        tube('Orange window surround', side_points + side_points[:1], .010, m['orange'], root)
        tube('Window black seal', [(x+side*.003,y,z) for x,y,z in side_points+side_points[:1]], .005, m['black'], root)
        tube('B pillar', [(side*.404,-.403,.641),(side*.448,-.435,.457)], .012, m['black'], root)
        # Lower door shut line, side sill and front fender air outlet.
        tube('Door shut', [(side*.512,.266,.403),(side*.519,.285,.177),(side*.514,.15,.149),(side*.523,-.34,.149),(side*.536,-.439,.215),(side*.537,-.43,.414)], .0018, m['black'], root)
        mesh.add_box('Side skirt', (.047,1.055,.090),(side*.523,-.03,.102),m['carbon'],root,bevel=.01)
        tube('Orange sill pinstripe', [(side*.55,.455,.148),(side*.55,-.48,.148)], .006, m['orange'], root)
        mesh.add_box('Door handle', (.010,.082,.017),(side*.512,-.237,.395),m['silver'],root,bevel=.006)
        mesh.add_box('Fender outlet', (.012,.160,.052),(side*.530,.426,.353),m['black'],root,bevel=.015)
        for z in [.341,.356,.371]:
            mesh.add_box('Fender outlet slat',(.015,.139,.003),(side*.538,.426,z),m['silver'],root,bevel=.001)
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
    surface('Orange bonnet stripe',pts,[(2*i,2*i+1,2*i+3,2*i+2) for i in range(len(rows)-1)],m['orange'],root)
    for side in [-1,1]:
        tube('Bonnet stripe navy keyline',[(side*(w+.004),y,z+.004) for y,z,w in rows],.003,m['navy'],root)
        # Recessed cooling slots and bonnet catches.
        for y,z in [(.60,.455),(.68,.469),(.76,.467)]:
            panel('Bonnet vent',[(side*.254+dx,y+dy,z) for dx,dy in [(-.055,-.02),(.055,-.02),(.055,.02),(-.055,.02)]],m['black'],root)
        for x,y,z in [(side*.340,.981,.428),(side*.280,.342,.421)]:
            disc('Bonnet fastener',(x,y,z+.003),.010,m['silver'],root,up=(0,1,0))

    # Top windscreen sunstrip and a single endurance wiper.
    panel('Orange sunstrip',[(-.346,.009,.668),(.346,.009,.668),(.360,.059,.624),(-.360,.059,.624)],m['orange'],root)
    text('Windscreen banner','youngDriver',(0,.087,.622),.037,m['white'],root,right=(-1,0,0),up=(0,-.67,.74))
    tube('Windscreen wiper',[(.21,.247,.444),(.045,.151,.555),(-.115,.100,.597)],.004,m['carbon'],root)
    tube('Roof aerial',[(.045,-.345,.709),(.045,-.377,.950)],.0025,m['black'],root)
    mesh.add_box('Roof camera',(.039,.050,.022),(-.061,-.304,.708),m['carbon'],root,bevel=.005)


def nose_and_tail(root,m):
    # Aston's wide, pinched-waist grille with four yellow endurance lamps.
    rim=[(-.356,1.231,.344),(-.398,1.257,.288),(-.346,1.272,.180),(.346,1.272,.180),(.398,1.257,.288),(.356,1.231,.344)]
    panel('Grille dark opening',rim,m['black'],root)
    tube('Orange grille rim',rim+rim[:1],.012,m['orange'],root)
    for x in [-.32,-.29,-.26,-.23,-.20,-.17,-.14,-.11,-.08,-.05,-.02,.01,.04,.07,.10,.13,.16,.19,.22,.25,.28,.31]:
        tube('Grille mesh vertical',[(x,1.273,.201),(x,1.258,.323)],.0015,m['mesh'],root)
    for z in [.21,.23,.25,.27,.29,.31]:
        tube('Grille mesh horizontal',[(-.337,1.274,z),(.337,1.274,z)],.0015,m['mesh'],root)
    for x in [-.137,-.048,.048,.137]:
        mesh.add_cylinder('Endurance light housing',.043,.032,(x,1.284,.259),m['carbon'],root,segments=32,rotation=(math.pi/2,0,0))
        disc('Yellow endurance lens',(x,1.303,.259),.036,m['yellow'],root)
        tube('Lens detail',[(x-.027,1.304,.250),(x+.027,1.304,.250)],.0015,m['amber'],root)
    # Swept teardrop headlights sit on the top corners of the nose.
    for side in [-1,1]:
        outline=[(side*.315,1.151,.369),(side*.463,1.097,.387),(side*.484,.990,.416),(side*.389,1.041,.407)]
        panel('Headlamp carbon bucket',outline,m['black'],root)
        tube('Headlamp silver rim',outline+outline[:1],.004,m['silver'],root)
        for x,y,z in [(side*.393,1.102,.397),(side*.440,1.060,.410)]:
            disc('Headlight projector',(x,y,z),.026,m['yellow'],root,up=(0,-.88,.47))
        tube('Headlight LED',[(side*.339,1.141,.379),(side*.434,1.112,.396),(side*.467,1.055,.414)],.003,m['white'],root)
    splitter=[(-.57,1.135,.065),(-.49,1.292,.065),(-.28,1.332,.064),(.28,1.332,.064),(.49,1.292,.065),(.57,1.135,.065)]
    panel('Carbon front splitter',splitter,m['carbon'],root)
    tube('Splitter leading edge',splitter,.012,m['carbon'],root)
    mesh.add_box('Lower radiator',(.662,.025,.041),(0,1.256,.117),m['black'],root,bevel=.011)
    tube('Lower orange rim',[(-.34,1.274,.137),(0,1.282,.138),(.34,1.274,.137)],.006,m['orange'],root)
    for side in [-1,1]:
        for z in [.155,.198]:
            panel('Front dive plane',[(side*.485,1.164,z),(side*.587,1.112,z+.018),(side*.548,.962,z-.018)],m['carbon'],root)
    # Aston wing badge, deliberately drawn as original geometry.
    panel('Wing badge',[(-.068,1.196,.374),(-.043,1.210,.373),(0,1.211,.373),(.043,1.210,.373),(.068,1.196,.374),(.018,1.194,.379),(0,1.197,.380),(-.018,1.194,.379)],m['silver'],root)
    text('Badge name','ASTON MARTIN',(0,1.202,.381),.010,m['navy'],root,right=(-1,0,0),up=(0,-1,0))

    # Tail lamps, rear valance and visible diffuser fins are important in chase view.
    for side in [-1,1]:
        shape=[(side*.482,-1.138,.371),(side*.270,-1.206,.344),(side*.265,-1.214,.307),(side*.472,-1.162,.325)]
        panel('Tail lamp surround',shape,m['black'],root)
        tube('Red tail signature',[(side*.468,-1.149,.361),(side*.298,-1.209,.336),(side*.287,-1.215,.322),(side*.449,-1.176,.335)],.009,m['red'],root)
        disc('Rear marker',(side*.420,-1.210,.256),.017,m['red'],root,right=(1,0,0))
    mesh.add_box('Rear mesh valance',(.765,.028,.103),(0,-1.214,.207),m['black'],root,bevel=.02)
    for x in [-.34,-.25,-.16,-.07,.07,.16,.25,.34]:
        tube('Rear grille slot',[(x,-1.232,.175),(x,-1.232,.239)],.002,m['mesh'],root)
    panel('Diffuser tray',[(-.47,-.99,.059),(.47,-.99,.059),(.49,-1.271,.130),(-.49,-1.271,.130)],m['carbon'],root)
    for x in [-.44,-.29,-.14,0,.14,.29,.44]:
        panel('Diffuser strake',[(x,-.96,.062),(x,-1.270,.137),(x,-1.266,.052),(x,-1.07,.038)],m['carbon'],root)
    tube('Rear orange pinstripe',[(-.48,-1.181,.139),(0,-1.243,.140),(.48,-1.181,.139)],.006,m['orange'],root)
    mesh.add_box('Rear rain light',(.055,.014,.029),(0,-1.252,.166),m['red'],root,bevel=.004)
    # Two mounts and a real, cambered aerofoil across the rear.
    for x in [-.293,.293]:
        panel('Wing upright',[(x,-.963,.398),(x,-1.114,.430),(x,-1.116,.720),(x,-1.043,.714)],m['carbon'],root)
        mesh.add_box('Wing foot',(.078,.166,.016),(x,-1.019,.422),m['carbon'],root,bevel=.006)
    wing=[]
    for x in [-.632,.632]:
        for y,z in [(-.984,.735),(-1.020,.751),(-1.164,.748),(-1.226,.731),(-1.217,.721),(-1.036,.730)]:
            wing.append((x,y,z))
    faces=[tuple(range(5,-1,-1)),tuple(range(6,12))]+[(i,(i+1)%6,(i+1)%6+6,i+6) for i in range(6)]
    surface('Rear wing aerofoil',wing,faces,m['carbon'],root,True)
    for side in [-1,1]:
        panel('Wing endplate',[(side*.636,-.960,.657),(side*.636,-.960,.785),(side*.636,-1.239,.785),(side*.636,-1.254,.658)],m['carbon'],root)
        text('Wing Hackett','HACKETT',(side*.638,-1.101,.740),.032,m['white'],root,right=(0,side,0))
        text('Wing London','LONDON',(side*.638,-1.101,.702),.017,m['white'],root,right=(0,side,0))
    text('Wing top lettering','ASTON MARTIN RACING',(0,-1.103,.753),.063,m['white'],root,right=(1,0,0),up=(0,1,0))
    text('Rear Aston lettering','ASTON MARTIN',(0,-1.220,.291),.032,m['silver'],root,right=(1,0,0))
    text('Rear racing number','95',(0,-1.237,.210),.067,m['white'],root,right=(1,0,0))


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
            text('Tyre Michelin '+tag,'MICHELIN',(side*.085,yoff,zoff),.018,m['white'],hub,right=(0,side,0))


def livery(root,m):
    for side in [-1,1]:
        # Number board, with the correct orange AM field and white digits.
        x=side*.540
        panel('Number board',[(x,.242,.163),(x,-.015,.163),(x,-.015,.402),(x,.242,.402)],m['white'],root)
        panel('Orange number field',[(x+side*.001,.228,.182),(x+side*.001,.000,.182),(x+side*.001,.000,.347),(x+side*.001,.228,.347)],m['orange'],root)
        panel('Le Mans blue header',[(x+side*.002,.231,.350),(x+side*.002,-.002,.350),(x+side*.002,-.002,.376),(x+side*.002,.231,.376)],m['navy'],root)
        text('Door number','95',(x+side*.004,.114,.267),.176,m['white'],root,right=(0,side,0))
        text('Le Mans header','24h LE MANS',(x+side*.004,.114,.362),.018,m['white'],root,right=(0,side,0))
        text('GTE class','AM',(x+side*.004,.114,.393),.024,m['orange'],root,right=(0,side,0))
        text('WEC footer','WEC',(x+side*.004,.114,.170),.020,m['navy'],root,right=(0,side,0))
        for title,y,z,size in [('SCANVO',-.240,.344,.059),('Basecamp.com',-.256,.271,.029),('JAEGER-LECOULTRE',-.246,.187,.016),('HASSELBLAD',.418,.400,.021),('JYSKE BANK',-.73,.447,.020)]:
            text('Sponsor '+title,title,(side*.550,y,z),size,m['navy'],root,right=(0,side,0))
        for y in [-.075,-.13,-.185]:
            disc('AM position indicator',(side*.551,y,.222),.006,m['amber'],root,right=(0,side,0))
        for title,y,z in [('D. HEINEMEIER HANSSON',-.17,.676),('N. THIIM / K. POULSEN',-.24,.700)]:
            text('Driver names',title,(side*.404,y,z),.010,m['navy'],root,right=(0,side,0))
        # Gulf roundel on the front bumper and rear quarter.
        for y,z,xpos in [(1.056,.238,.498),(-1.052,.319,.533)]:
            r=(0,side,0)
            center=(side*xpos,y,z)
            disc('Gulf white badge',center,.036,m['white'],root,right=r)
            disc('Gulf orange badge',(center[0]+side*.001,y,z),.029,m['orange'],root,right=r)
            text('Gulf logo','Gulf',(center[0]+side*.003,y,z),.033,m['navy'],root,right=r)
        text('Michelin sill','MICHELIN',(side*.551,.364,.107),.024,m['white'],root,right=(0,side,0))
    # Bonnet number and roundel read clearly from elevated gameplay views.
    disc('Bonnet roundel white',(0,1.010,.434),.075,m['white'],root,up=(0,1,0))
    disc('Bonnet roundel orange',(0,1.010,.435),.063,m['orange'],root,up=(0,1,0))
    text('Bonnet Gulf','Gulf',(0,1.010,.437),.067,m['navy'],root,right=(-1,0,0),up=(0,-1,0))
    text('Bonnet 95','95',(0,.464,.453),.118,m['white'],root,right=(-1,0,0),up=(0,-1,0))
    text('Bonnet young driver','youngDriver',(0,.331,.431),.021,m['navy'],root,right=(-1,0,0),up=(0,-1,0))
    text('Roof identity','95',(0,-.255,.714),.143,m['navy'],root,up=(0,1,0))
    # Small Danish cross at the roof's rear edge.
    panel('Danish flag',[(.13,-.41,.694),(.24,-.41,.694),(.24,-.46,.678),(.13,-.46,.678)],m['red'],root)
    panel('Danish cross vertical',[(.165,-.41,.695),(.174,-.41,.695),(.174,-.46,.679),(.165,-.46,.679)],m['white'],root)
    panel('Danish cross horizontal',[(.13,-.431,.688),(.24,-.431,.688),(.24,-.440,.685),(.13,-.440,.685)],m['white'],root)


def fit_details(root):
    """Lay glass and decals onto the finished, subdivided body surfaces."""
    body = bpy.data.objects['Vantage sculpted body']
    cabin = bpy.data.objects['Fastback cabin']
    bpy.context.view_layer.update()
    for obj in list(root.children_recursive):
        if obj.type != 'MESH':
            continue
        name = obj.name
        direction = Vector((0, 0, -1))
        target = None
        offset = .003
        if name.startswith(('Front windscreen', 'Rear windscreen', 'Window seal', 'Orange sunstrip', 'Windscreen banner', 'Windscreen wiper', 'Roof identity', 'Danish')):
            target = cabin
            if name.startswith(('Window seal', 'Danish cross')):
                offset = .005
            elif name.startswith('Orange sunstrip'):
                offset = .007
            elif name.startswith('Windscreen banner'):
                offset = .010
        elif name.startswith(('Side glazing', 'Orange window surround', 'Window black seal', 'B pillar')):
            target = cabin
            direction = Vector((-1 if obj.data.vertices[0].co.x > 0 else 1, 0, 0))
            if name.startswith('Orange window surround'):
                offset = .006
            elif name.startswith(('Window black seal', 'B pillar')):
                offset = .009
        elif name.startswith(('Orange bonnet stripe', 'Bonnet stripe', 'Bonnet roundel', 'Bonnet Gulf', 'Bonnet 95', 'Bonnet young', 'Bonnet vent', 'Wing badge', 'Badge name', 'Headlamp', 'Headlight')):
            target = body
            if name.startswith(('Bonnet roundel white', 'Headlamp carbon')):
                offset = .005
            elif name.startswith(('Bonnet roundel orange', 'Headlamp silver')):
                offset = .006
            elif name.startswith(('Bonnet Gulf', 'Bonnet 95', 'Bonnet young', 'Headlight', 'Badge name')):
                offset = .008
        elif name.startswith(('Number board', 'Orange number field', 'Le Mans blue header', 'Door number', 'Le Mans header', 'GTE class', 'WEC footer', 'Sponsor', 'AM position', 'Gulf white', 'Gulf orange', 'Gulf logo')):
            target = body
            center = obj.matrix_world @ obj.data.vertices[0].co
            direction = Vector((-1 if center.x > 0 else 1, 0, 0))
            if name.startswith(('Orange number field', 'Le Mans blue header', 'Gulf orange')):
                offset = .005
            elif name.startswith(('Door number', 'Le Mans header', 'GTE class', 'WEC footer', 'Gulf logo')):
                offset = .007
        if target is None:
            continue
        # A decal must follow the surface between its corners too. Subdivide
        # broad patches before projection so the curved body cannot poke through.
        if name.startswith(('Front windscreen', 'Rear windscreen', 'Side glazing', 'Orange bonnet stripe', 'Orange sunstrip', 'Number board', 'Orange number field', 'Le Mans blue header', 'Bonnet roundel', 'Gulf white', 'Gulf orange', 'Headlamp carbon')):
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


def join_render_parts(root):
    """One multi-material mesh for the body and one per rotating wheel."""
    hubs = [obj for obj in root.children if obj.name in ['WheelFL', 'WheelFR', 'WheelRL', 'WheelRR']]
    for parent in [root, *hubs]:
        parts = [obj for obj in parent.children if obj.type == 'MESH']
        bpy.ops.object.select_all(action='DESELECT')
        for obj in parts:
            obj.select_set(True)
        bpy.context.view_layer.objects.active = parts[0]
        bpy.ops.object.join()
        obj = bpy.context.object
        obj.name = 'Vantage bodywork' if parent == root else 'Racing slick and rim ' + parent.name[-2:]
        obj.data.name = obj.name
        # Joining retains the active object's transform; bake it into the mesh,
        # leaving all four wheel origins exactly at the physics contact points.
        bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)


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
    root=bpy.data.objects.new('Aston Martin V8 Vantage GTE 95 - Le Mans 2014',None)
    bpy.context.collection.objects.link(root)
    m={
        'paint':mesh.principled('Paint',base=(.448,.674,.750,1),metallic=.28,roughness=.26),
        'orange':mesh.principled('Gulf orange',base=(1,.135,.008,1),roughness=.33),
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
    studio()
    bpy.context.preferences.filepaths.save_version=0
    bpy.ops.wm.save_as_mainfile(filepath=str(BLEND))
    for name,path in [('Front',PREVIEW),('Rear',Path('/tmp/todora-vantage-rear.png')),('Side',Path('/tmp/todora-vantage-side.png'))]:
        bpy.context.scene.camera=bpy.data.objects[name]
        bpy.context.scene.render.filepath=str(path)
        bpy.ops.render.render(write_still=True)
    print(f'Wrote {BLEND}, {GLB} and {PREVIEW}')


if __name__=='__main__':
    main()
