"""Modeling additions: extrude options, primitives with booleans, scale, split, offset face / press pull, delete, STEP import, projection."""
import sys, json, math
import os, sys
# Playwright for Python: `pip install playwright && playwright install chromium`, or point PYLIBS at a site-packages folder.
if os.environ.get('PYLIBS'): sys.path.insert(0, os.environ['PYLIBS'])
REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), '..', '..')) + '/'
from playwright.sync_api import sync_playwright
OUT = REPO + 'doc/'
BASE = sys.argv[1] if len(sys.argv) > 1 else 'http://127.0.0.1:8080/'
errors, fails = [], []
def check(name, cond, info=''):
    print(('PASS' if cond else 'FAIL'), name, info)
    if not cond: fails.append(name)
with sync_playwright() as p:
    b = p.chromium.launch(args=['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--no-sandbox'])
    pg = b.new_page(viewport={'width': 1600, 'height': 1000})
    pg.on('pageerror', lambda e: errors.append('pageerror: ' + str(e)))
    pg.on('console', lambda m: errors.append('console.error: ' + m.text) if m.type == 'error' and '404' not in m.text else None)
    pg.goto(BASE + '?theme=dark&lang=en'); pg.wait_for_timeout(4000)
    vol = lambda i=0: pg.evaluate(f'cadrust.status().bodies[{i}].volume')
    nbodies = lambda: pg.evaluate('cadrust.status().bodies.length')
    v0, n0 = vol(), nbodies()
    nfeat = lambda: len(pg.evaluate('cadrust.doc().timeline'))

    # ribbon has the new tools enabled; the lower-priority ones sit in the group drop-down only (as in Fusion)
    for tool in ['Sphere', 'Torus', 'OffsetFace', 'Scale', 'Split', 'Delete', 'Import']:
        check(f'ribbon has {tool}', pg.is_enabled(f'button.tool[data-tool={tool}]'))
    check('press pull enabled', pg.is_enabled('#btnPressPull'))
    pg.click('.rlabel[data-menu=create]'); pg.wait_for_timeout(150)
    check('create drop-down lists the primitives', pg.is_visible('#groupMenu .mi >> text=Sphere') and pg.is_visible('#groupMenu .mi >> text=Torus'))
    pg.keyboard.press('Escape')
    def start(tool):
        if pg.is_visible(f'button.tool[data-tool={tool}]'): pg.click(f'button.tool[data-tool={tool}]')
        else:
            grp = 'create' if tool in ('Cylinder', 'Sphere', 'Torus') else 'modify'
            pg.click(f'.rlabel[data-menu={grp}]'); pg.wait_for_timeout(150)
            pg.click(f'#groupMenu .mi:has-text("{pg.evaluate("t => cadrust.label(t)", tool)}")')

    # sphere cut into the bracket (boolean on a primitive)
    start('Sphere'); pg.wait_for_timeout(200)
    pg.fill('#dialog [data-k=radius] input', '8')
    pg.fill('#dialog [data-k=at] input >> nth=0', '30'); pg.fill('#dialog [data-k=at] input >> nth=1', '20'); pg.fill('#dialog [data-k=at] input >> nth=2', '6')
    pg.select_option('#dialog [data-k=op] select', 'Cut'); pg.wait_for_timeout(100)
    check('target row appears for Cut', pg.is_visible('#dialog [data-k=target]'))
    pg.click('#dOk'); pg.wait_for_timeout(1800)
    check('sphere cut reduces volume, no new body', vol() < v0 - 100 and nbodies() == n0, str((v0, vol(), nbodies())))
    pg.click('#btnUndo'); pg.wait_for_timeout(1500)

    # torus as a new body, then scale it x2 about its centre (volume x8)
    start('Torus'); pg.wait_for_timeout(200)
    pg.fill('#dialog [data-k=at] input >> nth=0', '100'); pg.click('#dOk'); pg.wait_for_timeout(1800)
    check('torus new body', nbodies() == n0 + 1, str(nbodies()))
    tv = vol(n0)
    expect = 2 * math.pi ** 2 * 20 * 25
    check('torus volume', abs(tv - expect) < 1, str((tv, expect)))
    start('Scale'); pg.wait_for_timeout(200)
    pg.select_option('#dialog [data-k=body] select', 'torus1'); pg.fill('#dialog [data-k=factor] input', '2'); pg.fill('#dialog [data-k=center] input >> nth=0', '100')
    pg.click('#dOk'); pg.wait_for_timeout(1800)
    check('scale x2 gives 8x volume', abs(vol(n0) - 8 * tv) < 1, str(vol(n0)))
    check('timeline shows torus and scale', [f['type'] for f in pg.evaluate('cadrust.doc().timeline')][-2:] == ['Torus', 'Scale'])

    # delete the torus
    start('Delete'); pg.wait_for_timeout(200)
    pg.select_option('#dialog [data-k=body] select', 'torus1'); pg.click('#dOk'); pg.wait_for_timeout(1800)
    check('delete removes the body', nbodies() == n0, str(nbodies()))
    for _ in range(3): pg.click('#btnUndo'); pg.wait_for_timeout(600)
    pg.wait_for_timeout(1200)
    check('undo back to start', nbodies() == n0 and abs(vol() - v0) < 1e-6 and nfeat() == 10, str((nbodies(), vol(), nfeat())))

    # split the bracket by XY at z = 3: two bodies
    start('Split'); pg.wait_for_timeout(200)
    pg.fill('#dialog [data-k=offset] input', '3'); pg.click('#dOk'); pg.wait_for_timeout(1800)
    ids = pg.evaluate('cadrust.status().bodies.map(b => b.id)')
    check('split makes two bodies', nbodies() == n0 + 1 and 'split1' in ids, str(ids))
    check('split volumes add up', abs(sum(pg.evaluate('cadrust.status().bodies.map(b => b.volume)')) - v0) < 1e-3)
    pg.screenshot(path=OUT + 'ui_split_body.png')
    pg.click('#btnUndo'); pg.wait_for_timeout(1500)

    # offset face: a 20 mm box at x=200, its top face pulled up 4 mm (volume 8000 -> 9600)
    pg.click('button.tool[data-tool=Box]'); pg.wait_for_timeout(200)
    pg.fill('#dialog [data-k=at] input >> nth=0', '200'); pg.click('#dOk'); pg.wait_for_timeout(1800)
    bi = nbodies() - 1
    start('OffsetFace'); pg.wait_for_timeout(200)
    check('offset face starts in pick mode', pg.inner_text('#dialog .picks').startswith('No faces'))
    pg.evaluate("cadrust.pickFace('box1', [210, 10, 20])"); pg.wait_for_timeout(100)
    check('face pick counted', pg.inner_text('#dialog .picks').startswith('1 face'), pg.inner_text('#dialog .picks'))
    check('face pick sets the body', pg.input_value('#dialog [data-k=body] select') == 'box1')
    pg.fill('#dialog [data-k=distance] input', '4'); pg.click('#dOk'); pg.wait_for_timeout(1800)
    st = pg.evaluate('cadrust.status().status.at(-1)')
    check('offset face ok', st['ok'] and abs(vol(bi) - 9600) < 1e-3, json.dumps(st) + ' ' + str(vol(bi)))
    names = pg.evaluate('cadrust.doc().timeline.at(-1).faces.map(f => f.name)')
    check('picked face got its persistent name', names == ['box1/z+'], str(names))
    pg.screenshot(path=OUT + 'ui_offset_face.png')
    pg.click('#btnUndo'); pg.wait_for_timeout(1500)

    # press pull: with a body + face selected it opens Offset Face preset on that face
    pg.evaluate("cadrust.select('box1', [210, 10, 20])")
    pg.keyboard.press('q'); pg.wait_for_timeout(200)
    check('press pull on a face opens Offset Face', 'OFFSET FACE' in pg.inner_text('#dialog .dh').upper() and pg.inner_text('#dialog .picks').startswith('1 face'), pg.inner_text('#dialog .dh'))
    pg.click('#dCancel')
    pg.evaluate("cadrust.select(null, null)")
    pg.keyboard.press('q'); pg.wait_for_timeout(200)
    check('press pull with nothing selected opens Extrude', 'EXTRUDE' in pg.inner_text('#dialog .dh').upper())
    pg.click('#dCancel'); pg.click('#btnUndo'); pg.wait_for_timeout(1500)
    check('box undone', nbodies() == n0)
    pg.keyboard.press('q'); pg.wait_for_timeout(200)
    # extrude dialog options: symmetric hides distance2; Cut shows through all
    check('symmetric row present', pg.is_visible('#dialog [data-k=symmetric]'))
    check('through all hidden for NewBody', not pg.is_visible('#dialog [data-k=through_all]'))
    pg.select_option('#dialog [data-k=op] select', 'Cut'); pg.wait_for_timeout(100)
    check('through all shown for Cut', pg.is_visible('#dialog [data-k=through_all]'))
    pg.click('#dCancel')

    # edit the existing extrude: symmetric 3 mm each side on the base sketch
    pg.click('#timeline .feat >> nth=1'); pg.dblclick('#timeline .feat >> nth=1'); pg.wait_for_timeout(300)
    check('edit extrude dialog', 'EXTRUDE' in pg.inner_text('#dialog .dh').upper())
    pg.check('#dialog [data-k=symmetric] input'); pg.wait_for_timeout(100)
    check('distance2 hidden when symmetric', not pg.is_visible('#dialog [data-k=distance2]'))
    pg.click('#dOk'); pg.wait_for_timeout(1800)
    bb = pg.evaluate('cadrust.status().bodies[0]')
    check('symmetric extrude spans both sides', bb['bbox_min'][2] < -1 and pg.evaluate('cadrust.doc().timeline[1].symmetric') is True, str(bb['bbox_min']))
    pg.click('#btnUndo'); pg.wait_for_timeout(1500)

    # tapered extrude on a fresh box sketch via JSON (dialog field round trip)
    pg.click('#timeline .feat >> nth=1'); pg.dblclick('#timeline .feat >> nth=1'); pg.wait_for_timeout(300)
    pg.fill('#dialog [data-k=taper] input', '-8'); pg.click('#dOk'); pg.wait_for_timeout(2000)
    st = pg.evaluate('cadrust.status().status[1]')
    check('tapered extrude evaluates', st['ok'] and vol() < v0, json.dumps(st) + ' ' + str((v0, vol())))
    pg.click('#btnUndo'); pg.wait_for_timeout(1500)
    # a small tapered part for the docs: symmetric draft extrude of a slotted plate, loaded as its own design
    saved = pg.evaluate('cadrust.doc()')
    taper_doc = { 'name': 'taper', 'parameters': { 'w': '60', 'draft': '-12' }, 'timeline': [
        { 'type': 'Sketch', 'id': 'sk', 'plane': 'XY', 'entities': [ { 'type': 'Rect', 'x': 0, 'y': 0, 'w': 'w', 'h': '40' }, { 'type': 'Circle', 'cx': 20, 'cy': 20, 'r': 6 }, { 'type': 'Circle', 'cx': 40, 'cy': 20, 'r': 6 } ] },
        { 'type': 'Extrude', 'id': 'body', 'sketch': 'sk', 'distance': 14, 'symmetric': True, 'taper': 'draft' } ] }
    pg.evaluate('d => cadrust.loadDoc(d, "taper")', taper_doc); pg.wait_for_timeout(2500)
    st = pg.evaluate('cadrust.status().status[1]')
    check('taper demo evaluates', st['ok'] and nbodies() == 1, json.dumps(st))
    pg.click('#timeline .feat >> nth=1'); pg.dblclick('#timeline .feat >> nth=1'); pg.wait_for_timeout(400)
    check('taper dialog shows the expression', pg.input_value('#dialog [data-k=taper] input') == 'draft' and pg.is_checked('#dialog [data-k=symmetric] input'))
    pg.screenshot(path=OUT + 'ui_taper_extrude.png')
    pg.click('#dCancel')
    pg.evaluate('d => cadrust.loadDoc(d, "bracket")', saved); pg.wait_for_timeout(2500)
    check('bracket restored', pg.inner_text('#docName') == 'bracket' and abs(vol() - v0) < 1e-6)

    # STEP import: export the current model as STEP, import it back as a new body
    step = pg.evaluate("cadrust.exportBytes('step')")
    pg.evaluate("([n, a]) => cadrust.importFile(n, new Uint8Array(a))", ['bracket.step', step]); pg.wait_for_timeout(2500)
    check('import dialog opened with the file name', pg.is_visible('#dialog') and 'bracket.step' in pg.inner_text('#dialog'), pg.inner_text('#dialog .dh'))
    pg.fill('#dialog [data-k=at] input >> nth=0', '80'); pg.click('#dOk'); pg.wait_for_timeout(2500)
    st = pg.evaluate('cadrust.status().status.at(-1)')
    check('imported body evaluates', st['ok'] and st['kind'] == 'Import' and nbodies() == n0 + 1, json.dumps(st))
    check('imported volume matches', abs(vol(n0) - v0) < 1e-3, str((vol(n0), v0)))
    check('document keeps the geometry', len(pg.evaluate('cadrust.doc().timeline.at(-1).data')) > 1000)
    pg.screenshot(path=OUT + 'ui_import_step.png')
    pg.click('#btnUndo'); pg.wait_for_timeout(1500)

    # orthographic projection toggle via the display menu
    pg.click('#btnDisplay'); pg.click('#dispMenu [data-cam=ortho]'); pg.wait_for_timeout(300)
    check('orthographic projection', pg.evaluate('cadrust.projection()') == 'ortho')
    pg.evaluate("cadrust.view('top')"); pg.wait_for_timeout(200)
    pg.click('#btnDisplay'); pg.click('#dispMenu [data-cam=persp]'); pg.wait_for_timeout(300)
    check('back to perspective', pg.evaluate('cadrust.projection()') == 'persp')

    # combine keep tool
    pg.click('button.tool[data-tool=Box]'); pg.wait_for_timeout(200)
    pg.fill('#dialog [data-k=at] input >> nth=0', '50'); pg.click('#dOk'); pg.wait_for_timeout(1800)
    pg.click('button.tool[data-tool=Combine]'); pg.wait_for_timeout(200)
    pg.select_option('#dialog [data-k=target] select', 'base'); pg.select_option('#dialog [data-k=tool] select', 'box1')
    pg.check('#dialog [data-k=keep_tool] input'); pg.click('#dOk'); pg.wait_for_timeout(1800)
    check('combine keeps the tool body', nbodies() == n0 + 1 and pg.evaluate('cadrust.doc().timeline.at(-1).keep_tool') is True, str(nbodies()))

    check('no console errors', not errors, '\n'.join(errors[:5]))
    b.close()
print('FAILS', fails)
